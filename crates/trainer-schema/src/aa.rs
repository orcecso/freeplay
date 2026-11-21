use std::collections::HashMap;
use std::thread;
use std::time::Duration;

use iced_x86::code_asm::{
    dword_ptr, qword_ptr, AsmMemoryOperand, AsmRegister32, AsmRegister64, AsmRegisterXmm,
    CodeAssembler, CodeLabel,
};
use trainer_core::{AobPattern, BytePatch, ProcessHandle, ScanOptions};

use crate::error::{Result, SchemaError};
use crate::schema::{parse_hex_bytes, parse_hex_isize, parse_hex_usize};

/// Live activation handle for an Auto Assembler script.
/// Holds all allocated caves, applied injection-site patches, and published symbols so
/// `deactivate()` cleanly restores original bytes and frees remote memory.
pub struct ActiveAutoAssembler {
    patches: Vec<BytePatch>,
    allocs: Vec<usize>,
    pub registered_symbols: HashMap<String, usize>,
}

impl ActiveAutoAssembler {
    pub fn deactivate(&mut self, process: &ProcessHandle) {
        // 1. Restore original bytes at all injection sites first.
        for patch in self.patches.iter_mut().rev() {
            if patch.is_applied() {
                let _ = patch.revert(process);
            }
        }
        // 2. Give any in-flight game thread a moment to exit the cave before freeing it.
        if !self.allocs.is_empty() {
            thread::sleep(Duration::from_millis(15));
            for addr in self.allocs.drain(..) {
                let _ = process.free_memory(addr);
            }
        }
        self.registered_symbols.clear();
    }
}

#[derive(Debug)]
struct Section {
    name: String,
    base_addr: usize,
    is_cave: bool,
    lines: Vec<String>,
}

/// Executes the `[ENABLE]` block of a Cheat Engine x64 Auto Assembler script against `process`.
pub fn activate_auto_assembler(
    process: &ProcessHandle,
    script: &str,
) -> Result<ActiveAutoAssembler> {
    let lower = script.to_ascii_lowercase();
    if lower.contains("{$lua}") || lower.contains("luacall(") {
        return Err(SchemaError::AutoAssembler(
            "Lua blocks inside Auto Assembler scripts are not supported for security".into(),
        ));
    }

    let stripped = strip_curly_comments(script);
    let enable = extract_enable_section(&stripped).ok_or_else(|| {
        SchemaError::AutoAssembler("missing [ENABLE] section".into())
    })?;

    let mut symbols: HashMap<String, usize> = HashMap::new(); // lowercase key -> addr
    let mut sym_case: HashMap<String, String> = HashMap::new(); // lowercase -> original case
    let mut allocs: HashMap<String, usize> = HashMap::new();
    let mut alloc_addrs: Vec<usize> = Vec::new();
    let mut to_register: Vec<String> = Vec::new();
    let mut body_lines: Vec<String> = Vec::new();

    // Pass 1: run top-level directives (aobscan*, alloc, label, registersymbol)
    let pass1_res: Result<()> = (|| {
        for raw in enable.lines() {
            let line = raw.split("//").next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let lc = line.to_ascii_lowercase();

            if let Some(args) = strip_call_prefix(line, "aobscanmodule") {
                let parts: Vec<&str> = args.splitn(3, ',').map(str::trim).collect();
                if parts.len() != 3 {
                    return Err(SchemaError::AutoAssembler(format!(
                        "invalid aobscanmodule: {line}"
                    )));
                }
                let sym = parts[0];
                let module = strip_quotes(parts[1]);
                let pat = AobPattern::parse(parts[2])?;
                let hits = process.scan_module_aob(&module, &pat, true)?;
                let addr = hits.into_iter().next().ok_or_else(|| {
                    SchemaError::AutoAssembler(format!(
                        "aobscanmodule({sym}, {module}) found no matches"
                    ))
                })?;
                insert_sym(&mut symbols, &mut sym_case, sym, addr);
                continue;
            }

            if let Some(args) = strip_call_prefix(line, "aobscan") {
                let parts: Vec<&str> = args.splitn(2, ',').map(str::trim).collect();
                if parts.len() != 2 {
                    return Err(SchemaError::AutoAssembler(format!(
                        "invalid aobscan: {line}"
                    )));
                }
                let sym = parts[0];
                let pat = AobPattern::parse(parts[1])?;
                let hits = process.scan_aob(
                    &pat,
                    ScanOptions {
                        executable_only: true,
                        max_results: Some(1),
                        ..Default::default()
                    },
                )?;
                let addr = hits.into_iter().next().ok_or_else(|| {
                    SchemaError::AutoAssembler(format!("aobscan({sym}) found no matches"))
                })?;
                insert_sym(&mut symbols, &mut sym_case, sym, addr);
                continue;
            }

            if let Some(args) = strip_call_prefix(line, "alloc") {
                let parts: Vec<&str> = args.split(',').map(str::trim).collect();
                if parts.len() < 2 {
                    return Err(SchemaError::AutoAssembler(format!(
                        "invalid alloc: {line}"
                    )));
                }
                let name = parts[0];
                let size = parse_ce_size(parts[1])?;
                let near = if parts.len() >= 3 {
                    resolve_sym_expr(parts[2], &symbols).unwrap_or(0)
                } else {
                    0
                };
                let addr = process.alloc_near(near, size)?;
                alloc_addrs.push(addr);
                allocs.insert(name.to_ascii_lowercase(), addr);
                insert_sym(&mut symbols, &mut sym_case, name, addr);
                continue;
            }

            if let Some(args) = strip_call_prefix(line, "registersymbol") {
                for tok in args.split(|c: char| c.is_whitespace() || c == ',') {
                    let t = tok.trim();
                    if !t.is_empty() {
                        to_register.push(t.to_string());
                    }
                }
                continue;
            }

            if lc.starts_with("label(")
                || lc.starts_with("define(")
                || lc.starts_with("fullaccess(")
            {
                continue;
            }

            body_lines.push(line.to_string());
        }
        Ok(())
    })();

    if let Err(e) = pass1_res {
        for a in alloc_addrs {
            let _ = process.free_memory(a);
        }
        return Err(e);
    }

    // Pass 2: group lines into sections (allocated caves vs injection sites)
    let mut sections: Vec<Section> = Vec::new();
    for line in body_lines {
        if let Some(lbl) = line.strip_suffix(':') {
            let lbl_trim = lbl.trim();
            if let Some(addr) = resolve_sym_expr(lbl_trim, &symbols) {
                let is_cave = allocs.contains_key(&lbl_trim.to_ascii_lowercase());
                sections.push(Section {
                    name: lbl_trim.to_string(),
                    base_addr: addr,
                    is_cave,
                    lines: Vec::new(),
                });
                continue;
            }
        }
        if let Some(sec) = sections.last_mut() {
            sec.lines.push(line);
        } else {
            for a in alloc_addrs {
                let _ = process.free_memory(a);
            }
            return Err(SchemaError::AutoAssembler(format!(
                "instruction outside any section: '{line}'"
            )));
        }
    }

    // Pass 3a: assemble injection-site sections first so trailing `return:` labels resolve!
    let mut site_writes: Vec<(usize, Vec<u8>)> = Vec::new();
    for sec in sections.iter().filter(|s| !s.is_cave) {
        match assemble_section(process, sec, &mut symbols, &mut sym_case) {
            Ok(bytes) => {
                if !bytes.is_empty() {
                    site_writes.push((sec.base_addr, bytes));
                }
            }
            Err(e) => {
                for a in alloc_addrs {
                    let _ = process.free_memory(a);
                }
                return Err(e);
            }
        }
    }

    // Pass 3b: assemble allocated cave sections (now `return` and all site labels are known!)
    let mut cave_writes: Vec<(usize, Vec<u8>)> = Vec::new();
    for sec in sections.iter().filter(|s| s.is_cave) {
        match assemble_section(process, sec, &mut symbols, &mut sym_case) {
            Ok(bytes) => {
                if !bytes.is_empty() {
                    cave_writes.push((sec.base_addr, bytes));
                }
            }
            Err(e) => {
                for a in alloc_addrs {
                    let _ = process.free_memory(a);
                }
                return Err(e);
            }
        }
    }

    // Pass 4: write caves first, then apply injection-site patches atomically.
    for (addr, bytes) in cave_writes {
        if let Err(e) = process.write_bytes(addr, &bytes) {
            for a in alloc_addrs {
                let _ = process.free_memory(a);
            }
            return Err(SchemaError::Core(e));
        }
    }

    let mut applied_patches: Vec<BytePatch> = Vec::new();
    for (addr, bytes) in site_writes {
        let mut patch = BytePatch::new(addr, bytes);
        if let Err(e) = patch.apply(process) {
            for p in applied_patches.iter_mut().rev() {
                let _ = p.revert(process);
            }
            for a in alloc_addrs {
                let _ = process.free_memory(a);
            }
            return Err(SchemaError::Core(e));
        }
        applied_patches.push(patch);
    }

    let mut registered_symbols = HashMap::new();
    for req in to_register {
        let key = req.to_ascii_lowercase();
        if let Some(&addr) = symbols.get(&key) {
            registered_symbols.insert(req, addr);
        }
    }

    Ok(ActiveAutoAssembler {
        patches: applied_patches,
        allocs: alloc_addrs,
        registered_symbols,
    })
}

fn assemble_section(
    process: &ProcessHandle,
    sec: &Section,
    symbols: &mut HashMap<String, usize>,
    sym_case: &mut HashMap<String, String>,
) -> Result<Vec<u8>> {
    let mut asm = CodeAssembler::new(64)
        .map_err(|e| SchemaError::AutoAssembler(format!("CodeAssembler init: {e}")))?;

    // Pre-create labels for every internal `name:` line in this section so forward
    // references like `mov [Coin_Ptr], rax` and `mulss xmm7, [Mutiplier_Power]` resolve!
    let mut local_labels: HashMap<String, CodeLabel> = HashMap::new();
    let mut local_label_names: Vec<String> = Vec::new();
    for line in &sec.lines {
        if let Some(lbl) = line.strip_suffix(':') {
            let name = lbl.trim();
            let lc = name.to_ascii_lowercase();
            if !local_labels.contains_key(&lc) {
                local_labels.insert(lc, asm.create_label());
                local_label_names.push(name.to_string());
            }
        }
    }

    // Track byte positions of local labels via an auxiliary pass or post-encode lookup.
    // To know exact final addresses of local data labels (e.g. `Coin_Ptr`, `Mutiplier_Power`),
    // we record the instruction count at each label and also assemble with `assemble_options`.
    let mut label_instr_indices: Vec<(String, usize)> = Vec::new();

    for line in &sec.lines {
        if let Some(lbl) = line.strip_suffix(':') {
            let name = lbl.trim();
            let lc = name.to_ascii_lowercase();
            let code_lbl = local_labels.get_mut(&lc).unwrap();
            let instr_idx = asm.instructions().len();
            label_instr_indices.push((name.to_string(), instr_idx));
            asm.set_label(code_lbl)
                .map_err(|e| SchemaError::AutoAssembler(format!("set_label({name}): {e}")))?;
            continue;
        }

        emit_line(&mut asm, process, line, symbols, &local_labels)?;
    }

    // Add a sentinel NOP if the last item in the section was a label (like `return:`),
    // then strip that trailing sentinel byte after encoding!
    let ends_with_label = sec.lines.last().is_some_and(|l| l.ends_with(':'));
    if ends_with_label {
        asm.nop()
            .map_err(|e| SchemaError::AutoAssembler(format!("sentinel nop: {e}")))?;
    }

    let result = asm
        .assemble_options(
            sec.base_addr as u64,
            iced_x86::BlockEncoderOptions::RETURN_NEW_INSTRUCTION_OFFSETS,
        )
        .map_err(|e| {
            SchemaError::AutoAssembler(format!("assemble section '{}': {e}", sec.name))
        })?;

    let mut code_bytes = result.inner.code_buffer;
    if ends_with_label && !code_bytes.is_empty() {
        code_bytes.pop();
    }

    // Publish exact runtime addresses of every internal label (e.g. `Coin_Ptr`, `return`, `Mutiplier_Power`).
    for (name, instr_idx) in label_instr_indices {
        let addr = if instr_idx < result.inner.new_instruction_offsets.len() {
            sec.base_addr + result.inner.new_instruction_offsets[instr_idx] as usize
        } else {
            sec.base_addr + code_bytes.len()
        };
        insert_sym(symbols, sym_case, &name, addr);
    }

    Ok(code_bytes)
}

fn emit_line(
    asm: &mut CodeAssembler,
    process: &ProcessHandle,
    line: &str,
    symbols: &HashMap<String, usize>,
    local_labels: &HashMap<String, CodeLabel>,
) -> Result<()> {
    let lc = line.to_ascii_lowercase();

    // readmem(sym[+off], len)
    if let Some(args) = strip_call_prefix(line, "readmem") {
        let parts: Vec<&str> = args.split(',').map(str::trim).collect();
        if parts.len() != 2 {
            return Err(SchemaError::AutoAssembler(format!(
                "invalid readmem: {line}"
            )));
        }
        let addr = resolve_sym_expr(parts[0], symbols).ok_or_else(|| {
            SchemaError::AutoAssembler(format!("unresolved symbol in readmem: {}", parts[0]))
        })?;
        let len: usize = parts[1].parse().map_err(|_| {
            SchemaError::AutoAssembler(format!("invalid readmem len: {}", parts[1]))
        })?;
        let bytes = process.read_bytes(addr, len)?;
        asm.db(&bytes)
            .map_err(|e| SchemaError::AutoAssembler(format!("db(readmem): {e}")))?;
        return Ok(());
    }

    // db HH HH ...
    if let Some(rest) = strip_mnemonic(line, "db") {
        let bytes = parse_hex_bytes(rest)?;
        asm.db(&bytes)
            .map_err(|e| SchemaError::AutoAssembler(format!("db: {e}")))?;
        return Ok(());
    }

    // dd (float)N | dd N
    if let Some(rest) = strip_mnemonic(line, "dd") {
        let val = parse_u32_or_float(rest)?;
        asm.dd(&[val])
            .map_err(|e| SchemaError::AutoAssembler(format!("dd: {e}")))?;
        return Ok(());
    }

    // dq (float)N | dq (double)N | dq N
    if let Some(rest) = strip_mnemonic(line, "dq") {
        let val = parse_u64_or_typed(rest)?;
        asm.dq(&[val])
            .map_err(|e| SchemaError::AutoAssembler(format!("dq: {e}")))?;
        return Ok(());
    }

    // nop / nop N
    if lc == "nop" {
        asm.nop()
            .map_err(|e| SchemaError::AutoAssembler(format!("nop: {e}")))?;
        return Ok(());
    }
    if let Some(rest) = strip_mnemonic(line, "nop") {
        let count: usize = rest.trim().parse().map_err(|_| {
            SchemaError::AutoAssembler(format!("invalid nop count: {line}"))
        })?;
        for _ in 0..count {
            asm.nop()
                .map_err(|e| SchemaError::AutoAssembler(format!("nop: {e}")))?;
        }
        return Ok(());
    }

    if lc == "ret" {
        asm.ret()
            .map_err(|e| SchemaError::AutoAssembler(format!("ret: {e}")))?;
        return Ok(());
    }

    if let Some(rest) = strip_mnemonic(line, "push") {
        let reg = parse_reg64(rest)?;
        asm.push(reg)
            .map_err(|e| SchemaError::AutoAssembler(format!("push: {e}")))?;
        return Ok(());
    }

    if let Some(rest) = strip_mnemonic(line, "pop") {
        let reg = parse_reg64(rest)?;
        asm.pop(reg)
            .map_err(|e| SchemaError::AutoAssembler(format!("pop: {e}")))?;
        return Ok(());
    }

    if let Some(rest) = strip_mnemonic(line, "jmp") {
        let target = rest.trim();
        let target_lc = target.to_ascii_lowercase();
        if let Some(&lbl) = local_labels.get(&target_lc) {
            asm.jmp(lbl)
                .map_err(|e| SchemaError::AutoAssembler(format!("jmp label: {e}")))?;
        } else if let Some(addr) = resolve_sym_expr(target, symbols) {
            asm.jmp(addr as u64)
                .map_err(|e| SchemaError::AutoAssembler(format!("jmp addr: {e}")))?;
        } else {
            return Err(SchemaError::AutoAssembler(format!(
                "unresolved jmp target '{target}'"
            )));
        }
        return Ok(());
    }

    // Split `mnemonic dst, src`
    let (mnemonic, ops) = line
        .split_once(char::is_whitespace)
        .ok_or_else(|| SchemaError::AutoAssembler(format!("unsupported instruction: '{line}'")))?;
    let (dst_raw, src_raw) = ops
        .split_once(',')
        .ok_or_else(|| SchemaError::AutoAssembler(format!("expected 2 operands: '{line}'")))?;
    let dst = dst_raw.trim();
    let src = src_raw.trim();

    match mnemonic.to_ascii_lowercase().as_str() {
        "mov" => emit_mov(asm, dst, src, symbols, local_labels),
        "movss" | "movsd" | "mulss" | "mulsd" | "addss" | "addsd" | "subss" | "subsd"
        | "divss" | "divsd" => emit_sse(
            asm,
            &mnemonic.to_ascii_lowercase(),
            dst,
            src,
            symbols,
            local_labels,
        ),
        "xor" => {
            if let (Ok(r1), Ok(r2)) = (parse_reg64(dst), parse_reg64(src)) {
                asm.xor(r1, r2)
                    .map_err(|e| SchemaError::AutoAssembler(format!("xor: {e}")))?;
                return Ok(());
            }
            if let (Ok(r1), Ok(r2)) = (parse_reg32(dst), parse_reg32(src)) {
                asm.xor(r1, r2)
                    .map_err(|e| SchemaError::AutoAssembler(format!("xor: {e}")))?;
                return Ok(());
            }
            Err(SchemaError::AutoAssembler(format!(
                "unsupported xor operands: '{line}'"
            )))
        }
        _ => Err(SchemaError::AutoAssembler(format!(
            "unsupported instruction: '{line}'"
        ))),
    }
}

fn emit_mov(
    asm: &mut CodeAssembler,
    dst: &str,
    src: &str,
    symbols: &HashMap<String, usize>,
    local_labels: &HashMap<String, CodeLabel>,
) -> Result<()> {
    // 1. mov [mem], src
    if let Some(mem_inner) = strip_brackets(dst) {
        let mem_lc = mem_inner.to_ascii_lowercase();
        // mov [local_label], reg64
        if let Some(&lbl) = local_labels.get(&mem_lc) {
            let r = parse_reg64(src)?;
            asm.mov(qword_ptr(lbl), r)
                .map_err(|e| SchemaError::AutoAssembler(format!("mov [lbl], r64: {e}")))?;
            return Ok(());
        }
        // mov [reg+disp], ...
        let mem = parse_mem_operand(mem_inner)?;
        if let Ok(r) = parse_reg64(src) {
            asm.mov(mem, r)
                .map_err(|e| SchemaError::AutoAssembler(format!("mov [mem], r64: {e}")))?;
            return Ok(());
        }
        if let Ok(r) = parse_reg32(src) {
            asm.mov(mem, r)
                .map_err(|e| SchemaError::AutoAssembler(format!("mov [mem], r32: {e}")))?;
            return Ok(());
        }
        // mov [reg+disp], (float)1  or  mov [reg+disp], 123
        let imm = parse_u32_or_float(src)?;
        asm.mov(dword_ptr_from(mem), imm)
            .map_err(|e| SchemaError::AutoAssembler(format!("mov dword [mem], imm32: {e}")))?;
        return Ok(());
    }

    // 2. mov reg64, [mem]
    if let Ok(r_dst) = parse_reg64(dst) {
        if let Some(mem_inner) = strip_brackets(src) {
            let mem_lc = mem_inner.to_ascii_lowercase();
            if let Some(&lbl) = local_labels.get(&mem_lc) {
                asm.mov(r_dst, qword_ptr(lbl))
                    .map_err(|e| SchemaError::AutoAssembler(format!("mov r64, [lbl]: {e}")))?;
                return Ok(());
            }
            if let Some(addr) = resolve_sym_expr(mem_inner, symbols) {
                asm.mov(r_dst, qword_ptr(addr as u64))
                    .map_err(|e| SchemaError::AutoAssembler(format!("mov r64, [sym]: {e}")))?;
                return Ok(());
            }
            let mem = parse_mem_operand(mem_inner)?;
            asm.mov(r_dst, mem)
                .map_err(|e| SchemaError::AutoAssembler(format!("mov r64, [mem]: {e}")))?;
            return Ok(());
        }
        if let Ok(r_src) = parse_reg64(src) {
            asm.mov(r_dst, r_src)
                .map_err(|e| SchemaError::AutoAssembler(format!("mov r64, r64: {e}")))?;
            return Ok(());
        }
        let imm = parse_u64_or_typed(src)?;
        asm.mov(r_dst, imm)
            .map_err(|e| SchemaError::AutoAssembler(format!("mov r64, imm64: {e}")))?;
        return Ok(());
    }

    // 3. mov reg32, reg32 | imm32
    if let Ok(r_dst) = parse_reg32(dst) {
        if let Ok(r_src) = parse_reg32(src) {
            asm.mov(r_dst, r_src)
                .map_err(|e| SchemaError::AutoAssembler(format!("mov r32, r32: {e}")))?;
            return Ok(());
        }
        let imm = parse_u32_or_float(src)?;
        asm.mov(r_dst, imm)
            .map_err(|e| SchemaError::AutoAssembler(format!("mov r32, imm32: {e}")))?;
        return Ok(());
    }

    Err(SchemaError::AutoAssembler(format!(
        "unsupported mov: mov {dst}, {src}"
    )))
}

fn emit_sse(
    asm: &mut CodeAssembler,
    op: &str,
    dst: &str,
    src: &str,
    symbols: &HashMap<String, usize>,
    local_labels: &HashMap<String, CodeLabel>,
) -> Result<()> {
    // Store: movss/movsd [mem], xmm
    if let Some(mem_inner) = strip_brackets(dst) {
        let xmm = parse_xmm(src)?;
        let mem = resolve_sse_mem(mem_inner, op.ends_with("sd"), symbols, local_labels)?;
        match op {
            "movss" => asm.movss(mem, xmm),
            "movsd" => asm.movsd_2(mem, xmm),
            _ => {
                return Err(SchemaError::AutoAssembler(format!(
                    "{op} cannot store directly to memory"
                )))
            }
        }
        .map_err(|e| SchemaError::AutoAssembler(format!("{op} [mem], xmm: {e}")))?;
        return Ok(());
    }

    let x_dst = parse_xmm(dst)?;
    if let Ok(x_src) = parse_xmm(src) {
        match op {
            "movss" => asm.movss(x_dst, x_src),
            "movsd" => asm.movsd_2(x_dst, x_src),
            "mulss" => asm.mulss(x_dst, x_src),
            "mulsd" => asm.mulsd(x_dst, x_src),
            "addss" => asm.addss(x_dst, x_src),
            "addsd" => asm.addsd(x_dst, x_src),
            "subss" => asm.subss(x_dst, x_src),
            "subsd" => asm.subsd(x_dst, x_src),
            "divss" => asm.divss(x_dst, x_src),
            "divsd" => asm.divsd(x_dst, x_src),
            _ => unreachable!(),
        }
        .map_err(|e| SchemaError::AutoAssembler(format!("{op} xmm, xmm: {e}")))?;
        return Ok(());
    }

    if let Some(mem_inner) = strip_brackets(src) {
        let mem = resolve_sse_mem(mem_inner, op.ends_with("sd"), symbols, local_labels)?;
        match op {
            "movss" => asm.movss(x_dst, mem),
            "movsd" => asm.movsd_2(x_dst, mem),
            "mulss" => asm.mulss(x_dst, mem),
            "mulsd" => asm.mulsd(x_dst, mem),
            "addss" => asm.addss(x_dst, mem),
            "addsd" => asm.addsd(x_dst, mem),
            "subss" => asm.subss(x_dst, mem),
            "subsd" => asm.subsd(x_dst, mem),
            "divss" => asm.divss(x_dst, mem),
            "divsd" => asm.divsd(x_dst, mem),
            _ => unreachable!(),
        }
        .map_err(|e| SchemaError::AutoAssembler(format!("{op} xmm, [mem]: {e}")))?;
        return Ok(());
    }

    Err(SchemaError::AutoAssembler(format!(
        "unsupported SSE instruction: {op} {dst}, {src}"
    )))
}

fn resolve_sse_mem(
    inner: &str,
    is_double: bool,
    symbols: &HashMap<String, usize>,
    local_labels: &HashMap<String, CodeLabel>,
) -> Result<AsmMemoryOperand> {
    let lc = inner.to_ascii_lowercase();
    if let Some(&lbl) = local_labels.get(&lc) {
        return Ok(if is_double {
            qword_ptr(lbl)
        } else {
            dword_ptr(lbl)
        });
    }
    if let Some(addr) = resolve_sym_expr(inner, symbols) {
        return Ok(if is_double {
            qword_ptr(addr as u64)
        } else {
            dword_ptr(addr as u64)
        });
    }
    let mem = parse_mem_operand(inner)?;
    Ok(if is_double {
        qword_ptr_from(mem)
    } else {
        dword_ptr_from(mem)
    })
}

fn dword_ptr_from(mem: AsmMemoryOperand) -> AsmMemoryOperand {
    dword_ptr(mem)
}

fn qword_ptr_from(mem: AsmMemoryOperand) -> AsmMemoryOperand {
    qword_ptr(mem)
}

fn parse_mem_operand(inner: &str) -> Result<AsmMemoryOperand> {
    let s = inner.trim();
    if let Some((base, disp)) = s.split_once('+') {
        let reg = parse_reg64(base.trim())?;
        let off = parse_hex_isize(disp.trim())? as i32;
        return Ok(reg + off);
    }
    if let Some((base, disp)) = s.split_once('-') {
        let reg = parse_reg64(base.trim())?;
        let off = parse_hex_isize(disp.trim())? as i32;
        return Ok(reg - off);
    }
    let reg = parse_reg64(s)?;
    Ok(reg + 0)
}

fn parse_reg64(s: &str) -> Result<AsmRegister64> {
    use iced_x86::code_asm::*;
    Ok(match s.trim().to_ascii_lowercase().as_str() {
        "rax" => rax,
        "rbx" => rbx,
        "rcx" => rcx,
        "rdx" => rdx,
        "rsi" => rsi,
        "rdi" => rdi,
        "rbp" => rbp,
        "rsp" => rsp,
        "r8" => r8,
        "r9" => r9,
        "r10" => r10,
        "r11" => r11,
        "r12" => r12,
        "r13" => r13,
        "r14" => r14,
        "r15" => r15,
        _ => return Err(SchemaError::AutoAssembler(format!("unknown r64 '{s}'"))),
    })
}

fn parse_reg32(s: &str) -> Result<AsmRegister32> {
    use iced_x86::code_asm::*;
    Ok(match s.trim().to_ascii_lowercase().as_str() {
        "eax" => eax,
        "ebx" => ebx,
        "ecx" => ecx,
        "edx" => edx,
        "esi" => esi,
        "edi" => edi,
        "ebp" => ebp,
        "esp" => esp,
        "r8d" => r8d,
        "r9d" => r9d,
        "r10d" => r10d,
        "r11d" => r11d,
        "r12d" => r12d,
        "r13d" => r13d,
        "r14d" => r14d,
        "r15d" => r15d,
        _ => return Err(SchemaError::AutoAssembler(format!("unknown r32 '{s}'"))),
    })
}

fn parse_xmm(s: &str) -> Result<AsmRegisterXmm> {
    use iced_x86::code_asm::*;
    Ok(match s.trim().to_ascii_lowercase().as_str() {
        "xmm0" => xmm0,
        "xmm1" => xmm1,
        "xmm2" => xmm2,
        "xmm3" => xmm3,
        "xmm4" => xmm4,
        "xmm5" => xmm5,
        "xmm6" => xmm6,
        "xmm7" => xmm7,
        "xmm8" => xmm8,
        "xmm9" => xmm9,
        "xmm10" => xmm10,
        "xmm11" => xmm11,
        "xmm12" => xmm12,
        "xmm13" => xmm13,
        "xmm14" => xmm14,
        "xmm15" => xmm15,
        _ => return Err(SchemaError::AutoAssembler(format!("unknown xmm '{s}'"))),
    })
}

fn strip_brackets(s: &str) -> Option<&str> {
    let t = s.trim();
    let t = t
        .strip_prefix("dword ptr ")
        .or_else(|| t.strip_prefix("qword ptr "))
        .or_else(|| t.strip_prefix("DWORD PTR "))
        .or_else(|| t.strip_prefix("QWORD PTR "))
        .unwrap_or(t)
        .trim();
    t.strip_prefix('[')?.strip_suffix(']')
}

fn parse_u32_or_float(s: &str) -> Result<u32> {
    let t = s.trim();
    if let Some(rest) = t
        .strip_prefix("(float)")
        .or_else(|| t.strip_prefix("(FLOAT)"))
    {
        let f: f32 = rest
            .trim()
            .parse()
            .map_err(|_| SchemaError::AutoAssembler(format!("invalid float '{t}'")))?;
        return Ok(f.to_bits());
    }
    if let Some(hex) = t.strip_prefix('$').or_else(|| t.strip_prefix("0x")) {
        return u32::from_str_radix(hex, 16)
            .map_err(|_| SchemaError::AutoAssembler(format!("invalid hex u32 '{t}'")));
    }
    t.parse::<u32>()
        .or_else(|_| u32::from_str_radix(t, 16))
        .map_err(|_| SchemaError::AutoAssembler(format!("invalid u32 '{t}'")))
}

fn parse_u64_or_typed(s: &str) -> Result<u64> {
    let t = s.trim();
    if let Some(rest) = t
        .strip_prefix("(float)")
        .or_else(|| t.strip_prefix("(FLOAT)"))
    {
        let f: f32 = rest
            .trim()
            .parse()
            .map_err(|_| SchemaError::AutoAssembler(format!("invalid float '{t}'")))?;
        // Cheat Engine `dq (float)2` stores the 32-bit IEEE float in the low 4 bytes + 4 zero bytes.
        return Ok(f.to_bits() as u64);
    }
    if let Some(rest) = t
        .strip_prefix("(double)")
        .or_else(|| t.strip_prefix("(DOUBLE)"))
    {
        let f: f64 = rest
            .trim()
            .parse()
            .map_err(|_| SchemaError::AutoAssembler(format!("invalid double '{t}'")))?;
        return Ok(f.to_bits());
    }
    if let Some(hex) = t.strip_prefix('$').or_else(|| t.strip_prefix("0x")) {
        return u64::from_str_radix(hex, 16)
            .map_err(|_| SchemaError::AutoAssembler(format!("invalid hex u64 '{t}'")));
    }
    t.parse::<u64>()
        .or_else(|_| u64::from_str_radix(t, 16))
        .map_err(|_| SchemaError::AutoAssembler(format!("invalid u64 '{t}'")))
}

fn parse_ce_size(s: &str) -> Result<usize> {
    let t = s.trim();
    if let Some(hex) = t.strip_prefix('$').or_else(|| t.strip_prefix("0x")) {
        return parse_hex_usize(hex);
    }
    t.parse::<usize>()
        .map_err(|_| SchemaError::AutoAssembler(format!("invalid alloc size '{t}'")))
}

fn insert_sym(
    symbols: &mut HashMap<String, usize>,
    sym_case: &mut HashMap<String, String>,
    name: &str,
    addr: usize,
) {
    let lc = name.to_ascii_lowercase();
    symbols.insert(lc.clone(), addr);
    sym_case.insert(lc, name.to_string());
}

fn resolve_sym_expr(expr: &str, symbols: &HashMap<String, usize>) -> Option<usize> {
    let t = expr.trim();
    if let Some((base, off)) = t.split_once('+') {
        let b = symbols.get(&base.trim().to_ascii_lowercase())?;
        let o = parse_hex_isize(off.trim()).ok()?;
        return Some(b.wrapping_add_signed(o));
    }
    if let Some((base, off)) = t.split_once('-') {
        let b = symbols.get(&base.trim().to_ascii_lowercase())?;
        let o = parse_hex_isize(off.trim()).ok()?;
        return Some(b.wrapping_add_signed(-o));
    }
    symbols.get(&t.to_ascii_lowercase()).copied()
}

fn strip_mnemonic<'a>(line: &'a str, mnem: &str) -> Option<&'a str> {
    let lc = line.to_ascii_lowercase();
    let prefix = format!("{mnem} ");
    if lc.starts_with(&prefix) {
        Some(line[prefix.len()..].trim())
    } else {
        None
    }
}

fn strip_call_prefix<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let lc = line.to_ascii_lowercase();
    let prefix = format!("{name}(");
    if !lc.starts_with(&prefix) {
        return None;
    }
    let inner = &line[prefix.len()..];
    let end = inner.rfind(')')?;
    Some(inner[..end].trim())
}

fn strip_quotes(s: &str) -> String {
    let t = s.trim();
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

fn strip_curly_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut depth = 0usize;
    for ch in input.chars() {
        match ch {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out
}

fn extract_enable_section(script: &str) -> Option<&str> {
    let lower = script.to_ascii_lowercase();
    let start_idx = lower.find("[enable]")? + "[enable]".len();
    let rest = &script[start_idx..];
    let rest_lower = &lower[start_idx..];
    if let Some(end_idx) = rest_lower.find("[disable]") {
        Some(&rest[..end_idx])
    } else {
        Some(rest)
    }
}
