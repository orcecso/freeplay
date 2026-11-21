use trainer_schema::{
    export_ct_xml, import_ct_xml, AddressSpec, EntryKind, ImportOptions, PointerWidthSchema,
    TrainerManifest, ValueTypeSchema,
};

#[test]
fn sample_json_manifest_validates_and_roundtrips() {
    let raw = include_str!("../../../examples/sample.trainer.json");
    let manifest = TrainerManifest::from_json_str(raw).expect("sample manifest should be valid");
    assert_eq!(manifest.entries.len(), 3);

    let serialized = manifest.to_json_pretty().unwrap();
    let reparsed = TrainerManifest::from_json_str(&serialized).unwrap();
    assert_eq!(manifest, reparsed);
}

#[test]
fn validation_rejects_duplicate_entry_ids_and_bad_patterns() {
    let bad_dup = r#"{
        "schema_version": 1,
        "id": "bad",
        "game": { "title": "T", "processes": ["t.exe"] },
        "author": "a",
        "version": "1.0.0",
        "entries": [
            { "id": "hp", "label": "HP", "kind": "value", "address": { "mode": "absolute", "address": "0x1000" }, "value_type": "i32" },
            { "id": "hp", "label": "HP2", "kind": "value", "address": { "mode": "absolute", "address": "0x1004" }, "value_type": "i32" }
        ]
    }"#;
    assert!(TrainerManifest::from_json_str(bad_dup).is_err());

    let bad_aob = r#"{
        "schema_version": 1,
        "id": "bad2",
        "game": { "title": "T", "processes": ["t.exe"] },
        "author": "a",
        "version": "1.0.0",
        "entries": [
            { "id": "p", "label": "Patch", "kind": "aob_patch", "pattern": "?? ??", "patch_bytes": "90" }
        ]
    }"#;
    assert!(TrainerManifest::from_json_str(bad_aob).is_err());
}

#[test]
fn import_ct_xml_handles_groups_reversed_offsets_and_skips_scripts() {
    let ct_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<CheatTable CheatEngineTableVersion="45">
  <CheatEntries>
    <CheatEntry>
      <ID>1</ID>
      <Description>"Player Stats"</Description>
      <GroupHeader>1</GroupHeader>
      <CheatEntries>
        <CheatEntry>
          <ID>2</ID>
          <Description>"Health"</Description>
          <VariableType>4 Bytes</VariableType>
          <Address>"game.exe"+001A2B3C</Address>
          <Offsets>
            <Offset>28</Offset>
            <Offset>10</Offset>
          </Offsets>
        </CheatEntry>
        <CheatEntry>
          <ID>3</ID>
          <Description>"Stamina"</Description>
          <VariableType>Float</VariableType>
          <Address>00405000</Address>
        </CheatEntry>
        <CheatEntry>
          <ID>4</ID>
          <Description>"Dangerous Script"</Description>
          <VariableType>Auto Assembler Script</VariableType>
          <AssemblerScript>[ENABLE]
nop
[DISABLE]
</AssemblerScript>
        </CheatEntry>
      </CheatEntries>
    </CheatEntry>
  </CheatEntries>
  <LuaScript>print("should not run")</LuaScript>
</CheatTable>
"#;

    let res = import_ct_xml(
        ct_xml,
        ImportOptions {
            id: "my-game".into(),
            game_title: "My Game".into(),
            process_name: "game.exe".into(),
            author: "tester".into(),
            pointer_width: PointerWidthSchema::Bits64,
            allow_auto_assembler: false,
        },
    )
    .expect("import_ct_xml");

    assert_eq!(res.manifest.entries.len(), 2);
    assert_eq!(res.skipped.len(), 1);
    assert_eq!(res.skipped[0].label, "Dangerous Script");
    assert_eq!(res.warnings.len(), 1); // LuaScript warning

    let health = &res.manifest.entries[0];
    assert_eq!(health.label, "Health");
    assert_eq!(health.category.as_deref(), Some("Player Stats"));
    match &health.kind {
        EntryKind::Value {
            address,
            value_type,
            ..
        } => {
            assert_eq!(*value_type, ValueTypeSchema::I32);
            // Offsets reversed from CE bottom-to-top [28, 10] -> top-to-bottom [0x10, 0x28]
            assert_eq!(
                *address,
                AddressSpec::ModulePointerChain {
                    module: "game.exe".into(),
                    base_offset: "0x001A2B3C".into(),
                    offsets: vec!["0x10".into(), "0x28".into()],
                }
            );
        }
        other => panic!("unexpected kind: {other:?}"),
    }

    // Export back to .CT XML and re-import to confirm round-trip preserves pointer order
    let (exported_xml, export_skipped) = export_ct_xml(&res.manifest).expect("export_ct_xml");
    assert!(export_skipped.is_empty());

    let reimported = import_ct_xml(&exported_xml, ImportOptions::default()).expect("reimport");
    assert_eq!(reimported.manifest.entries.len(), 2);
    assert_eq!(reimported.manifest.entries[0].kind, res.manifest.entries[0].kind);
    assert_eq!(reimported.manifest.entries[1].kind, res.manifest.entries[1].kind);
}

#[test]
fn import_ct_extracts_pure_aob_byte_patches_and_skips_code_caves() {
    let ct_xml = r#"<?xml version="1.0" encoding="utf-8"?>
<CheatTable CheatEngineTableVersion="52">
  <CheatEntries>
    <CheatEntry>
      <ID>1</ID>
      <Description>"Infinite Re-Rolls"</Description>
      <VariableType>Auto Assembler Script</VariableType>
      <AssemblerScript>[ENABLE]
aobscanmodule(ReRoll,GameAssembly.dll,49 8B 48 70 48 85 C9 0F 84 ** ** ** **) // comment
registersymbol(ReRoll)
alloc(ReRoll_Save,8)
registersymbol(ReRoll_Save)

ReRoll_Save:
  readmem(ReRoll+12,8)
ReRoll+12:
  db 90 90 90 90 90 90 90 90

[DISABLE]
ReRoll+12:
  readmem(ReRoll_Save,8)
unregistersymbol(*)
dealloc(*)
{ block comment }
</AssemblerScript>
    </CheatEntry>
    <CheatEntry>
      <ID>2</ID>
      <Description>"Code Cave Multiplier"</Description>
      <VariableType>Auto Assembler Script</VariableType>
      <AssemblerScript>[ENABLE]
aobscanmodule(Power,GameAssembly.dll,F3 0F 10 B8 D8 00 00 00)
alloc(newmem,$100,Power)
newmem:
  mulss xmm7,[Mutiplier_Power]
  jmp return
Power:
  jmp newmem
[DISABLE]
</AssemblerScript>
    </CheatEntry>
  </CheatEntries>
</CheatTable>
"#;

    let res = import_ct_xml(ct_xml, ImportOptions::default()).unwrap();
    assert_eq!(res.manifest.entries.len(), 1);
    assert_eq!(res.skipped.len(), 1);
    assert_eq!(res.manifest.entries[0].label, "Infinite Re-Rolls");
    assert_eq!(
        res.manifest.entries[0].kind,
        EntryKind::AobPatch {
            module: Some("GameAssembly.dll".into()),
            pattern: "49 8B 48 70 48 85 C9 0F 84 ** ** ** **".into(),
            patch_offset: 0x12,
            patch_bytes: "90 90 90 90 90 90 90 90".into(),
            executable_only: true,
        }
    );

    // When opt-in Auto Assembler is enabled, code-cave scripts also import!
    let res_aa = import_ct_xml(
        ct_xml,
        ImportOptions {
            allow_auto_assembler: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(res_aa.manifest.entries.len(), 2);
    assert!(matches!(
        res_aa.manifest.entries[1].kind,
        EntryKind::AutoAssembler { .. }
    ));
}

#[cfg(windows)]
mod live_session_tests {
    use std::collections::HashMap;
    use std::io::{BufRead, BufReader};
    use std::path::PathBuf;
    use std::process::{Child, Command, Stdio};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    use trainer_core::{MemoryValue, ProcessHandle};
    use trainer_schema::{
        AddressSpec, EntryKind, GameTarget, PointerWidthSchema, ScalarValue, TrainerEntry,
        TrainerManifest, TrainerSession, ValueTypeSchema, CURRENT_SCHEMA_VERSION,
    };

    struct RunningTarget {
        child: Child,
        meta: HashMap<String, String>,
    }

    impl RunningTarget {
        fn spawn() -> Self {
            let mut exe = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            exe.pop();
            exe.pop();
            exe.push("target");
            exe.push("debug");
            exe.push("dummy-target.exe");

            let mut child = Command::new(&exe)
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .expect("spawn dummy-target");

            let stdout = child.stdout.take().unwrap();
            let mut meta = HashMap::new();
            for line in BufReader::new(stdout).lines() {
                let line = line.unwrap();
                if line.trim() == "READY" {
                    break;
                }
                if let Some((k, v)) = line.split_once('=') {
                    meta.insert(k.trim().to_string(), v.trim().to_string());
                }
            }
            Self { child, meta }
        }
    }

    impl Drop for RunningTarget {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    #[test]
    fn trainer_session_executes_declarative_manifest_against_dummy_target() {
        let target = RunningTarget::spawn();
        let pid: u32 = target.meta["PID"].parse().unwrap();
        let proc_handle = Arc::new(ProcessHandle::attach_pid(pid).unwrap());

        let manifest = TrainerManifest {
            schema_version: CURRENT_SCHEMA_VERSION,
            id: "dummy-target-test".into(),
            game: GameTarget {
                title: "Dummy Target".into(),
                processes: vec!["dummy-target.exe".into()],
                steam_app_id: None,
                game_version: Some("0.1.0".into()),
                pointer_width: PointerWidthSchema::Bits64,
            },
            author: "test".into(),
            version: "1.0.0".into(),
            description: None,
            entries: vec![
                TrainerEntry {
                    id: "gold".into(),
                    label: "Gold".into(),
                    category: Some("Player".into()),
                    description: None,
                    hotkey: None,
                    kind: EntryKind::Value {
                        address: AddressSpec::Absolute {
                            address: target.meta["BASE_ADDR"].clone(),
                            offsets: vec![
                                target.meta["STATS_PTR_OFFSET"].clone(),
                                target.meta["GOLD_OFFSET"].clone(),
                            ],
                        },
                        value_type: ValueTypeSchema::I32,
                        default_value: Some(ScalarValue::Int(50_000)),
                        freeze: false,
                        min: None,
                        max: None,
                        step: None,
                    },
                },
                TrainerEntry {
                    id: "health".into(),
                    label: "Health".into(),
                    category: Some("Player".into()),
                    description: None,
                    hotkey: None,
                    kind: EntryKind::Value {
                        address: AddressSpec::Absolute {
                            address: target.meta["BASE_ADDR"].clone(),
                            offsets: vec![
                                target.meta["STATS_PTR_OFFSET"].clone(),
                                target.meta["HEALTH_OFFSET"].clone(),
                            ],
                        },
                        value_type: ValueTypeSchema::I32,
                        default_value: Some(ScalarValue::Int(777)),
                        freeze: true,
                        min: None,
                        max: None,
                        step: None,
                    },
                },
                TrainerEntry {
                    id: "sig_patch".into(),
                    label: "Signature Patch".into(),
                    category: Some("Patches".into()),
                    description: None,
                    hotkey: None,
                    kind: EntryKind::AobPatch {
                        module: None,
                        pattern: "54 52 4E 52 13 37 ?? DE AA BB ?? DD 01 02 03 04".into(),
                        patch_offset: 0,
                        patch_bytes: "90 90 90 90".into(),
                        executable_only: false,
                    },
                },
            ],
        };

        let mut session = TrainerSession::new(Arc::clone(&proc_handle), manifest).unwrap();

        // Read & write gold via pointer chain
        assert_eq!(session.read_entry("gold").unwrap(), MemoryValue::I32(250));
        session
            .write_entry("gold", &ScalarValue::Int(50_000))
            .unwrap();
        assert_eq!(
            session.read_entry("gold").unwrap(),
            MemoryValue::I32(50_000)
        );

        // Freeze health at default_value (777)
        session.set_freeze("health", true, None).unwrap();
        assert!(session.is_frozen("health"));
        thread::sleep(Duration::from_millis(120));
        let hp = match session.read_entry("health").unwrap() {
            MemoryValue::I32(v) => v,
            other => panic!("unexpected: {other:?}"),
        };
        assert!((775..=777).contains(&hp), "expected frozen hp near 777, got {hp}");

        // Toggle AoB patch on & off
        session.toggle_patch("sig_patch", true).unwrap();
        assert!(session.is_patch_active("sig_patch"));
        session.toggle_patch("sig_patch", false).unwrap();
        assert!(!session.is_patch_active("sig_patch"));

        session.disable_all();
        assert!(!session.is_frozen("health"));
    }
}
