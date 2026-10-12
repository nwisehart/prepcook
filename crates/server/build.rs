use std::{fs, path::Path};

fn main() {
    // Load .env from the workspace root
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let env_file = workspace_root.join(".env");
    dotenv::from_path(&env_file).ok();

    // Re-run build script when these env vars or .env file change
    println!("cargo:rerun-if-env-changed=POSTHOG_API_KEY");
    println!("cargo:rerun-if-env-changed=POSTHOG_API_ENDPOINT");
    println!("cargo:rerun-if-env-changed=PREPCOOK_SHARED_API_BASE");
    println!("cargo:rerun-if-env-changed=SENTRY_DSN");
    if env_file.exists() {
        println!("cargo:rerun-if-changed={}", env_file.display());
    }

    if let Ok(api_key) = std::env::var("POSTHOG_API_KEY") {
        println!("cargo:rustc-env=POSTHOG_API_KEY={}", api_key);
    }
    if let Ok(api_endpoint) = std::env::var("POSTHOG_API_ENDPOINT") {
        println!("cargo:rustc-env=POSTHOG_API_ENDPOINT={}", api_endpoint);
    }
    if let Ok(prepcook_shared_api_base) = std::env::var("PREPCOOK_SHARED_API_BASE") {
        println!(
            "cargo:rustc-env=PREPCOOK_SHARED_API_BASE={}",
            prepcook_shared_api_base
        );
    }

    // Create packages/local-web/dist if it doesn't exist. Anchored on
    // CARGO_MANIFEST_DIR, not the process cwd: RustEmbed resolves its
    // `#[folder]` against the manifest, so a cwd-relative path here can put the
    // placeholder somewhere the derive never looks — outside the repository
    // entirely — and a fresh worktree then cannot compile `server` at all until
    // the frontend has been built.
    let dist_path = workspace_root.join("packages/local-web/dist");
    // Declare the bundle as a rerun trigger even when it is absent, which makes
    // cargo run this script until it exists. Build-script output is cached per
    // target directory and a pool of worktrees shares one, so otherwise cargo
    // replays another worktree's output — the warning below included — into a
    // worktree where the placeholder was never created, and `server` fails to
    // compile on a path whose log says it succeeded.
    println!("cargo:rerun-if-changed={}", dist_path.display());
    if !dist_path.exists() {
        println!("cargo:warning=Creating dummy packages/local-web/dist directory for compilation");
        fs::create_dir_all(&dist_path).unwrap();

        // Create a dummy index.html
        let dummy_html = r#"<!DOCTYPE html>
<html><head><title>Build web app first</title></head>
<body><h1>Please build @prepcook/local-web first</h1></body></html>"#;

        fs::write(dist_path.join("index.html"), dummy_html).unwrap();
    }
}
