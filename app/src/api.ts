import { invoke } from "@tauri-apps/api/core";

const IS_DEMO =
  typeof window !== "undefined" && new URLSearchParams(window.location.search).get("demo") === "1";

const DEMO_REPOS: RepoSource[] = [
  {
    repo_id: "open-trainer-official",
    repo_name: "Open Trainer Community Hub",
    index_url: "https://raw.githubusercontent.com/open-trainer/community-repo/main/index.json",
    enabled: true,
  },
  {
    repo_id: "indie-roguelikes",
    repo_name: "Indie & Roguelike Tables",
    index_url: "https://raw.githubusercontent.com/community/roguelike-tables/main/index.json",
    enabled: true,
  },
];

const DEMO_INSTALLED: InstalledRecord[] = [
  {
    trainer_id: "warhammer-survivors",
    title: "Warhammer Survivors",
    version: "2.1.4",
    author: "community",
    processes: ["WarhammerSurvivors.exe"],
    source_repo_id: "open-trainer-official",
    sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
  },
  {
    trainer_id: "hollow-knight-silksong",
    title: "Example Single-Player RPG",
    version: "1.0.0",
    author: "alice",
    processes: ["example-rpg.exe"],
    source_repo_id: "indie-roguelikes",
    sha256: "8f434346648f6b96df89dda901c5176b10a6d83961dd3c1ac88b59b2dc327aa4",
  },
  {
    trainer_id: "pixel-dungeon-ct",
    title: "Pixel Dungeon (Imported .CT)",
    version: "1.2.0",
    author: "local-user",
    processes: ["PixelDungeon.exe"],
    sha256: "a591a6d40bf420404a011733cfb7b190d62c65bf0bcda32b57b277d9ad9f146e",
  },
];

const DEMO_MANIFEST: TrainerManifest = {
  schema_version: 1,
  id: "warhammer-survivors",
  game: {
    title: "Warhammer Survivors",
    processes: ["WarhammerSurvivors.exe"],
    steam_app_id: 2345670,
    game_version: "2.1.4",
    pointer_width: "64",
  },
  author: "community",
  version: "2.1.4",
  description: "Imported from Cheat Engine .CT with AoB patches and x64 Auto Assembler hooks.",
  entries: [
    {
      id: "invincible",
      label: "Invincible",
      category: "Player",
      hotkey: "F1",
      kind: "auto_assembler",
      description: "x64 Auto Assembler script (opt-in)",
    },
    {
      id: "infinite_health",
      label: "Infinite Health",
      category: "Player",
      hotkey: "F2",
      kind: "auto_assembler",
      description: "x64 Auto Assembler script (opt-in)",
    },
    {
      id: "current_coins",
      label: "Current Coins",
      category: "Find Coins",
      kind: "value",
      value_type: "f32",
      default_value: 99999,
    },
    {
      id: "xp_multiplier",
      label: "XP Gain Multiplier",
      category: "Multipliers",
      hotkey: "F4",
      kind: "auto_assembler",
      description: "x64 Auto Assembler script (opt-in)",
    },
    {
      id: "infinite_rerolls",
      label: "Infinite Re-Rolls",
      category: "Patches",
      hotkey: "F5",
      kind: "aob_patch",
      description: "Extracted declarative AoB byte patch from .CT",
    },
    {
      id: "infinite_banish",
      label: "Infinite Banish",
      category: "Patches",
      hotkey: "F6",
      kind: "aob_patch",
      description: "Extracted declarative AoB byte patch from .CT",
    },
  ],
};

export interface RepoSource {
  repo_id: string;
  repo_name: string;
  index_url: string;
  enabled: boolean;
}

export interface RepoTrainerItem {
  id: string;
  title: string;
  processes: string[];
  steam_app_id?: number;
  game_version?: string;
  trainer_version: string;
  author: string;
  path: string;
  sha256?: string;
  description?: string;
}

export interface RepoIndex {
  schema_version: number;
  repo_id: string;
  repo_name: string;
  description?: string;
  homepage?: string;
  trainers: RepoTrainerItem[];
}

export interface InstalledRecord {
  trainer_id: string;
  title: string;
  version: string;
  author: string;
  processes: string[];
  source_repo_id?: string;
  sha256: string;
}

export interface CatalogHit {
  repo_id: string;
  repo_name: string;
  item: RepoTrainerItem;
  installed_version?: string;
}

export interface AvailableUpdate {
  trainer_id: string;
  repo_id: string;
  title: string;
  installed_version: string;
  available_version: string;
}

export interface ProcessDto {
  pid: number;
  name: string;
  window_title?: string;
  exe_path?: string;
  is_system: boolean;
}

export interface TrainerEntry {
  id: string;
  label: string;
  category?: string;
  description?: string;
  hotkey?: string;
  kind: "value" | "aob_patch" | "auto_assembler";
  value_type?: string;
  extra_addresses?: unknown[];
  default_value?: number;
  freeze?: boolean;
  min?: number;
  max?: number;
  step?: number;
}

export interface TrainerManifest {
  schema_version: number;
  id: string;
  game: {
    title: string;
    processes: string[];
    steam_app_id?: number;
    game_version?: string;
    pointer_width: "32" | "64";
  };
  author: string;
  version: string;
  description?: string;
  entries: TrainerEntry[];
}

export interface SessionStatus {
  attached: boolean;
  trainer_id?: string;
  process_name?: string;
  pid?: number;
  alive: boolean;
}

export interface RefreshSummary {
  refreshed: number;
  errors: string[];
}

export interface CtImportSummary {
  record: InstalledRecord;
  imported_entries: number;
  skipped: string[];
  warnings: string[];
}

export interface CtExportSummary {
  xml: string;
  skipped: string[];
}

export type ValueTypeName = "u8" | "i16" | "u16" | "i32" | "u32" | "i64" | "u64" | "f32" | "f64";
export type ScanValueTypeName = "all" | ValueTypeName;

export interface ScanHitDto {
  address: string;
  value_type: ValueTypeName;
  value: string;
  previous?: string;
  static_ref?: string;
}

export interface ScanResponse {
  total_hits: number;
  preview: ScanHitDto[];
}

export interface CapturedEntryInput {
  label: string;
  category?: string;
  address: string;
  extra_addresses?: string[];
  value_type: ValueTypeName;
  default_value?: number;
  freeze: boolean;
}

export interface ScannerExportResult {
  trainer_id: string;
  trainer_json: string;
  sha256: string;
  ct_xml: string;
  repo_index_entry: string;
}

export const api = {
  listRepos: () => (IS_DEMO ? Promise.resolve(DEMO_REPOS) : invoke<RepoSource[]>("list_repos")),
  addRepo: (indexUrl: string) => invoke<RepoIndex>("add_repo", { indexUrl }),
  removeRepo: (repoId: string) => invoke<void>("remove_repo", { repoId }),
  refreshRepos: () => invoke<RefreshSummary>("refresh_repos"),
  searchCatalog: (query: string) =>
    IS_DEMO
      ? Promise.resolve<CatalogHit[]>(
          [
            {
              repo_id: "open-trainer-official",
              repo_name: "Open Trainer Community Hub",
              installed_version: "2.1.4",
              item: {
                id: "warhammer-survivors",
                title: "Warhammer Survivors",
                processes: ["WarhammerSurvivors.exe"],
                steam_app_id: 2345670,
                trainer_version: "2.1.4",
                author: "community",
                path: "trainers/warhammer-survivors.trainer.json",
                sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                description: "Invincibility, Infinite Health, Coins, Multipliers, Re-Rolls & Banish.",
              },
            },
            {
              repo_id: "indie-roguelikes",
              repo_name: "Indie & Roguelike Tables",
              installed_version: "1.0.0",
              item: {
                id: "hollow-knight-silksong",
                title: "Example Single-Player RPG",
                processes: ["example-rpg.exe"],
                steam_app_id: 123456,
                trainer_version: "1.1.0",
                author: "alice",
                path: "trainers/example-rpg.trainer.json",
                sha256: "8f434346648f6b96df89dda901c5176b10a6d83961dd3c1ac88b59b2dc327aa4",
                description: "Player Health, Gold, and No Stamina Drain.",
              },
            },
            {
              repo_id: "indie-roguelikes",
              repo_name: "Indie & Roguelike Tables",
              item: {
                id: "star-drifter",
                title: "Star Drifter",
                processes: ["StarDrifter.exe"],
                steam_app_id: 987654,
                trainer_version: "1.3.2",
                author: "kuro",
                path: "trainers/star-drifter.trainer.json",
                sha256: "2c26b46b68ffc68ff99b453c1d30413413422d706483bfa0f98a5e886266e7ae",
                description: "Infinite Fuel, Credits Editor, and Shield Freeze.",
              },
            },
          ].filter(
            (h) =>
              !query.trim() ||
              h.item.title.toLowerCase().includes(query.toLowerCase()) ||
              h.item.processes.some((p) => p.toLowerCase().includes(query.toLowerCase()))
          )
        )
      : invoke<CatalogHit[]>("search_catalog", { query }),
  checkUpdates: () =>
    IS_DEMO
      ? Promise.resolve<AvailableUpdate[]>([
          {
            trainer_id: "hollow-knight-silksong",
            repo_id: "indie-roguelikes",
            title: "Example Single-Player RPG",
            installed_version: "1.0.0",
            available_version: "1.1.0",
          },
        ])
      : invoke<AvailableUpdate[]>("check_updates"),

  listInstalled: () =>
    IS_DEMO ? Promise.resolve(DEMO_INSTALLED) : invoke<InstalledRecord[]>("list_installed"),
  loadTrainer: (trainerId: string) =>
    IS_DEMO
      ? Promise.resolve(DEMO_MANIFEST)
      : invoke<TrainerManifest>("load_trainer", { trainerId }),
  renameTrainerEntry: (trainerId: string, entryId: string, newLabel: string) => {
    if (IS_DEMO) {
      const target = DEMO_MANIFEST.entries.find((e) => e.id === entryId);
      if (target && newLabel.trim()) target.label = newLabel.trim();
      return Promise.resolve({ ...DEMO_MANIFEST, entries: [...DEMO_MANIFEST.entries] });
    }
    return invoke<TrainerManifest>("rename_trainer_entry", { trainerId, entryId, newLabel });
  },
  combineTrainerEntries: (trainerId: string, entryIds: string[]) =>
    invoke<TrainerManifest>("combine_trainer_entries", { trainerId, entryIds }),
  installTrainer: (repoId: string, trainerId: string) =>
    invoke<InstalledRecord>("install_trainer", { repoId, trainerId }),
  uninstallTrainer: (trainerId: string) => invoke<void>("uninstall_trainer", { trainerId }),

  importCt: (params: {
    xml: string;
    id: string;
    gameTitle: string;
    processName: string;
    author: string;
    pointerWidth: "32" | "64";
    allowAutoAssembler: boolean;
  }) => invoke<CtImportSummary>("import_ct", params),
  exportCt: (trainerId: string) => invoke<CtExportSummary>("export_ct", { trainerId }),
  importJsonManifest: (jsonText: string) =>
    invoke<InstalledRecord>("import_json_manifest", { jsonText }),

  listRunningProcesses: (includeSystem = false) =>
    IS_DEMO
      ? Promise.resolve<ProcessDto[]>([
          {
            pid: 23044,
            name: "WarhammerSurvivors.exe",
            window_title: "Warhammer Survivors",
            is_system: false,
          },
          {
            pid: 18420,
            name: "example-rpg.exe",
            window_title: "Example Single-Player RPG",
            is_system: false,
          },
        ])
      : invoke<ProcessDto[]>("list_running_processes", { includeSystem }),
  attachTrainer: (trainerId: string) =>
    IS_DEMO
      ? Promise.resolve<SessionStatus>({
          attached: true,
          trainer_id: trainerId,
          process_name: "WarhammerSurvivors.exe",
          pid: 23044,
          alive: true,
        })
      : invoke<SessionStatus>("attach_trainer", { trainerId }),
  attachTrainerToPid: (trainerId: string, pid: number, processName: string) =>
    invoke<SessionStatus>("attach_trainer_to_pid", { trainerId, pid, processName }),
  detachTrainer: () =>
    IS_DEMO
      ? Promise.resolve<SessionStatus>({ attached: false, alive: false })
      : invoke<SessionStatus>("detach_trainer"),
  sessionStatus: () =>
    IS_DEMO
      ? Promise.resolve<SessionStatus>({
          attached: true,
          trainer_id: "warhammer-survivors",
          process_name: "WarhammerSurvivors.exe",
          pid: 23044,
          alive: true,
        })
      : invoke<SessionStatus>("session_status"),
  readEntry: (entryId: string) =>
    IS_DEMO ? Promise.resolve("99999") : invoke<string>("read_entry", { entryId }),
  writeEntry: (entryId: string, value: number) =>
    IS_DEMO ? Promise.resolve() : invoke<void>("write_entry", { entryId, value }),
  setFreeze: (entryId: string, enabled: boolean, value?: number) =>
    IS_DEMO
      ? Promise.resolve(enabled)
      : invoke<boolean>("set_freeze", { entryId, enabled, value: value ?? null }),
  togglePatch: (entryId: string, enabled: boolean) =>
    IS_DEMO ? Promise.resolve(enabled) : invoke<boolean>("toggle_patch", { entryId, enabled }),

  scannerAttach: (pid: number, processName: string) =>
    IS_DEMO
      ? Promise.resolve(`${processName} (PID ${pid})`)
      : invoke<string>("scanner_attach", { pid, processName }),
  scannerDetach: () => invoke<void>("scanner_detach"),
  scannerFirstScan: (valueType: ScanValueTypeName, value: number) =>
    IS_DEMO
      ? Promise.resolve<ScanResponse>({
          total_hits: 3,
          preview: [
            {
              address: "0x7FF6A14028C4",
              value_type: "f32",
              value: String(value),
              static_ref: "GameAssembly.dll+0x4B28C4",
            },
            {
              address: "0x1E4B90C11084",
              value_type: "i32",
              value: String(value),
            },
            {
              address: "0x1E4B90C12400",
              value_type: "f64",
              value: String(value),
            },
          ],
        })
      : invoke<ScanResponse>("scanner_first_scan", { valueType, value }),
  scannerNextScan: (
    mode: "exact" | "increased" | "decreased" | "changed" | "unchanged",
    valueType: ScanValueTypeName,
    value?: number
  ) => invoke<ScanResponse>("scanner_next_scan", { mode, valueType, value: value ?? null }),
  scannerRefreshPreview: () => invoke<ScanResponse>("scanner_refresh_preview"),
  scannerWriteAddress: (address: string, valueType: ValueTypeName, value: number) =>
    IS_DEMO
      ? Promise.resolve()
      : invoke<void>("scanner_write_address", { address, valueType, value }),
  scannerExportTrainer: (params: {
    trainerId: string;
    gameTitle: string;
    processName: string;
    author: string;
    version: string;
    steamAppId?: number;
    description?: string;
    entries: CapturedEntryInput[];
    saveToLibrary: boolean;
  }) => invoke<ScannerExportResult>("scanner_export_trainer", params),

  openExternalUrl: async (url: string) => {
    try {
      await invoke<void>("open_external_url", { url });
    } catch {
      window.open(url, "_blank", "noopener,noreferrer");
    }
  },
  exitApp: () => invoke<void>("exit_app"),
};
