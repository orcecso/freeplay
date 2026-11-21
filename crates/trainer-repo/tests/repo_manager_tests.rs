use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use trainer_repo::{compute_sha256_hex, RepoError, RepoIndex, RepoManager, RepoTrainerItem};
use trainer_schema::{
    AddressSpec, EntryKind, GameTarget, PointerWidthSchema, TrainerEntry, TrainerManifest,
    ValueTypeSchema, CURRENT_SCHEMA_VERSION,
};

fn temp_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("freeplay_{prefix}_{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn sample_manifest(version: &str) -> TrainerManifest {
    TrainerManifest {
        schema_version: CURRENT_SCHEMA_VERSION,
        id: "sky-rpg".into(),
        game: GameTarget {
            title: "Sky RPG".into(),
            processes: vec!["skyrpg.exe".into()],
            steam_app_id: Some(424242),
            game_version: Some("1.2".into()),
            pointer_width: PointerWidthSchema::Bits64,
        },
        author: "alice".into(),
        version: version.into(),
        description: Some("Basic single-player values".into()),
        entries: vec![TrainerEntry {
            id: "gold".into(),
            label: "Gold".into(),
            category: Some("Inventory".into()),
            description: None,
            hotkey: Some("F1".into()),
            kind: EntryKind::Value {
                address: AddressSpec::ModulePointerChain {
                    module: "skyrpg.exe".into(),
                    base_offset: "0x1000".into(),
                    offsets: vec!["0x10".into()],
                },
                value_type: ValueTypeSchema::I32,
                default_value: None,
                freeze: false,
                min: None,
                max: None,
                step: None,
            },
        }],
    }
}

fn write_local_repo(
    repo_dir: &std::path::Path,
    trainer_version: &str,
    override_sha: Option<String>,
) -> PathBuf {
    std::fs::create_dir_all(repo_dir.join("trainers")).unwrap();
    let manifest_json = sample_manifest(trainer_version).to_json_pretty().unwrap();
    let sha = override_sha.unwrap_or_else(|| compute_sha256_hex(manifest_json.as_bytes()));

    std::fs::write(repo_dir.join("trainers").join("sky-rpg.json"), &manifest_json).unwrap();

    let index = RepoIndex {
        schema_version: 1,
        repo_id: "community-main".into(),
        repo_name: "Community Main".into(),
        description: Some("Test repo".into()),
        homepage: None,
        trainers: vec![RepoTrainerItem {
            id: "sky-rpg".into(),
            title: "Sky RPG".into(),
            processes: vec!["skyrpg.exe".into()],
            steam_app_id: Some(424242),
            game_version: Some("1.2".into()),
            trainer_version: trainer_version.into(),
            author: "alice".into(),
            path: "trainers/sky-rpg.json".into(),
            sha256: Some(sha),
            description: Some("Gold editor".into()),
        }],
    };
    let index_path = repo_dir.join("index.json");
    std::fs::write(&index_path, serde_json::to_string_pretty(&index).unwrap()).unwrap();
    index_path
}

#[test]
fn full_repo_lifecycle_install_search_update_and_uninstall() {
    let repo_dir = temp_dir("repo");
    let store_dir = temp_dir("store");

    let index_path = write_local_repo(&repo_dir, "1.0.0", None);
    let index_url = format!("file:///{}", index_path.to_string_lossy().replace('\\', "/"));

    let mgr = RepoManager::open(&store_dir).unwrap();

    // 1. Add repo
    let idx = mgr.add_repo(&index_url).unwrap();
    assert_eq!(idx.repo_id, "community-main");
    assert_eq!(mgr.list_repos().unwrap().len(), 1);

    // 2. Search by title, process, and Steam AppID
    assert_eq!(mgr.search_catalog("sky").unwrap().len(), 1);
    assert_eq!(mgr.search_catalog("skyrpg.exe").unwrap().len(), 1);
    assert_eq!(mgr.search_catalog("424242").unwrap().len(), 1);
    assert_eq!(mgr.search_catalog("nonexistent").unwrap().len(), 0);

    // 3. Install trainer (verifies SHA-256 + schema)
    let rec = mgr.install_trainer("community-main", "sky-rpg").unwrap();
    assert_eq!(rec.version, "1.0.0");
    assert_eq!(mgr.list_installed().unwrap().len(), 1);
    let loaded = mgr.load_installed("sky-rpg").unwrap();
    assert_eq!(loaded.game.title, "Sky RPG");

    // 4. Bump remote version to 1.1.0, refresh, and detect update
    write_local_repo(&repo_dir, "1.1.0", None);
    let (ok, errs) = mgr.refresh_all().unwrap();
    assert_eq!(ok.len(), 1);
    assert!(errs.is_empty());

    let updates = mgr.check_updates().unwrap();
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].installed_version, "1.0.0");
    assert_eq!(updates[0].available_version, "1.1.0");

    // 5. Upgrade by re-installing
    let upgraded = mgr.install_trainer("community-main", "sky-rpg").unwrap();
    assert_eq!(upgraded.version, "1.1.0");
    assert!(mgr.check_updates().unwrap().is_empty());

    // 6. Uninstall & remove repo
    mgr.uninstall_trainer("sky-rpg").unwrap();
    assert!(mgr.list_installed().unwrap().is_empty());
    mgr.remove_repo("community-main").unwrap();
    assert!(mgr.list_repos().unwrap().is_empty());

    let _ = std::fs::remove_dir_all(&repo_dir);
    let _ = std::fs::remove_dir_all(&store_dir);
}

#[test]
fn rejects_sha256_mismatch() {
    let repo_dir = temp_dir("repo_bad_sha");
    let store_dir = temp_dir("store_bad_sha");

    let bad_sha = "0".repeat(64);
    let index_path = write_local_repo(&repo_dir, "1.0.0", Some(bad_sha));
    let index_url = format!("file:///{}", index_path.to_string_lossy().replace('\\', "/"));

    let mgr = RepoManager::open(&store_dir).unwrap();
    mgr.add_repo(&index_url).unwrap();

    let err = mgr.install_trainer("community-main", "sky-rpg").unwrap_err();
    assert!(matches!(err, RepoError::ChecksumMismatch { .. }));

    let _ = std::fs::remove_dir_all(&repo_dir);
    let _ = std::fs::remove_dir_all(&store_dir);
}

#[test]
fn fetches_repo_and_trainer_over_http() {
    let store_dir = temp_dir("store_http");

    let manifest_json = sample_manifest("2.0.0").to_json_pretty().unwrap();
    let sha = compute_sha256_hex(manifest_json.as_bytes());
    let index = RepoIndex {
        schema_version: 1,
        repo_id: "http-repo".into(),
        repo_name: "HTTP Test Repo".into(),
        description: None,
        homepage: None,
        trainers: vec![RepoTrainerItem {
            id: "sky-rpg".into(),
            title: "Sky RPG".into(),
            processes: vec!["skyrpg.exe".into()],
            steam_app_id: Some(424242),
            game_version: None,
            trainer_version: "2.0.0".into(),
            author: "alice".into(),
            path: "trainers/sky-rpg.json".into(),
            sha256: Some(sha),
            description: None,
        }],
    };
    let index_json = serde_json::to_string_pretty(&index).unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let server = thread::spawn(move || {
        // Handle 2 sequential requests: GET /index.json and GET /trainers/sky-rpg.json
        for _ in 0..2 {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut req_buf = [0u8; 1024];
                let n = stream.read(&mut req_buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&req_buf[..n]);
                let body = if req.contains("GET /index.json") {
                    index_json.as_bytes()
                } else {
                    manifest_json.as_bytes()
                };
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(header.as_bytes());
                let _ = stream.write_all(body);
            }
        }
    });

    let mgr = RepoManager::open(&store_dir).unwrap();
    let url = format!("http://{addr}/index.json");
    mgr.add_repo(&url).unwrap();
    let rec = mgr.install_trainer("http-repo", "sky-rpg").unwrap();
    assert_eq!(rec.version, "2.0.0");

    let _ = server.join();
    let _ = std::fs::remove_dir_all(&store_dir);
}
