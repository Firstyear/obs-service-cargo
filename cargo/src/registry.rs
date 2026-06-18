use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;

use libroast::common::Compression;
use libroast::operations::cli::RoastArgs;
use libroast::operations::roast::roast_opts;
#[allow(unused_imports)]
use tracing::{Level, debug, error, info, trace, warn};

use crate::audit;
use crate::cargo_commands::*;
use crate::cli::Opts;
use crate::toml_manifest::has_dependencies;
use crate::toml_manifest::is_workspace;
use crate::toml_manifest::workspace_has_dependencies;

pub fn run_cargo_vendor_home_registry(
    setup_workdir: &Path,
    custom_root: &Path,
    registry: &Opts,
) -> io::Result<()> {
    debug!(?registry);
    info!("🛖🏃📦 Starting Cargo Vendor Home Registry");

    if registry.vendor_specific_args.is_some() {
        warn!(
            "🛑 The `--filter` and `--versioned-dirs flags are only used if `--method` is set to `vendor`."
        );
        warn!("Ignoring vendor specific flags...");
    }

    let tempdir_for_home_registry_binding = tempfile::Builder::new()
        .prefix(".cargo")
        .rand_bytes(12)
        .tempdir()?;

    let home_registry = tempdir_for_home_registry_binding.path();
    let home_registry_dot_cargo = home_registry.join(".cargo");
    let mut global_has_deps = false;

    unsafe {
        std::env::set_var("CARGO_HOME", &home_registry_dot_cargo);
    }

    let res = {
        debug!(?home_registry_dot_cargo);
        let no_root_manifest = registry.no_root_manifest.unwrap_or_default();
        if !no_root_manifest {
            let mut hash = blake3::Hasher::new();
            let possible_root_manifest = custom_root.join("Cargo.toml");
            if possible_root_manifest.is_file() {
                let is_workspace = is_workspace(&possible_root_manifest)?;
                let has_deps = has_dependencies(&possible_root_manifest)?;

                if is_workspace {
                    info!("ℹ️ This manifest is in WORKSPACE configuration.");
                    let workspace_has_deps =
                        workspace_has_dependencies(custom_root, &possible_root_manifest)?;
                    global_has_deps = workspace_has_deps || global_has_deps;
                    if !workspace_has_deps {
                        warn!(
                            "⚠️ This WORKSPACE MANIFEST does not seem to contain workspace dependencies and dev-dependencies. Please check member dependencies."
                        );
                    }
                } else if !has_deps {
                    info!("😄 This extra manifest does not seem to have any dependencies.");
                    info!(
                        "🙂 If you think this is a BUG 🐞, please open an issue at <https://github.com/openSUSE-Rust/obs-service-cargo/issues>."
                    );
                    if registry.manifest_path.is_empty() {
                        info!("🎉 No other manifests. No dependencies. Nothing to vendor.");
                    }
                }

                global_has_deps = has_deps || global_has_deps;

                let possible_root_manifest_parent = possible_root_manifest
                    .parent()
                    .unwrap_or(custom_root)
                    .canonicalize()?;

                let possible_lockfile = possible_root_manifest_parent.join("Cargo.lock");

                if possible_lockfile.is_file() {
                    let lockfile_bytes = fs::read(&possible_lockfile)?;
                    hash.update(&lockfile_bytes);
                    let output_hash = hash.finalize();
                    info!(?output_hash, "🔒 Lockfile hash before: ");
                } else {
                    let output_hash = hash.finalize();
                    info!(
                        ?output_hash,
                        "🔒 Lockfile not found. Showing default generated hash: "
                    );
                }

                cargo_update(
                    registry.update,
                    &registry.update_crate,
                    &possible_root_manifest_parent,
                    &possible_root_manifest.to_string_lossy(),
                    registry.respect_lockfile,
                )?;

                info!(?setup_workdir, "🌳 Finished setting up workdir.");
                info!("🚝 Attempting to fetch dependencies.");

                cargo_fetch(
                    &possible_root_manifest_parent,
                    &possible_root_manifest.to_string_lossy(),
                    registry.respect_lockfile,
                )?;

                if possible_lockfile.is_file() {
                    let lockfile_bytes = fs::read(&possible_lockfile)?;
                    hash.update(&lockfile_bytes);
                    let output_hash = hash.finalize();
                    info!(?output_hash, "🔒 Lockfile hash after: ");
                }

                info!("💼 Fetched dependencies.");
            }
        }

        let mut lockfiles: Vec<PathBuf> = Vec::new();

        for manifest in &registry.manifest_path {
            let mut hash = blake3::Hasher::new();
            if !manifest.ends_with("Cargo.toml") {
                let msg = format!(
                    "Expected a valid manifest filename. Got {}.",
                    manifest.display()
                );
                error!(?manifest, msg);
                return Err(io::Error::new(io::ErrorKind::InvalidInput, msg));
            }

            let full_manifest_path = &custom_root.join(manifest);
            let full_manifest_path_parent = full_manifest_path.parent().unwrap_or(custom_root);

            if full_manifest_path.is_file() {
                let is_workspace = is_workspace(full_manifest_path)?;
                let has_deps = has_dependencies(full_manifest_path)?;
                let possible_lockfile = full_manifest_path_parent.join("Cargo.lock");

                if possible_lockfile.is_file() {
                    let lockfile_bytes = fs::read(&possible_lockfile)?;
                    hash.update(&lockfile_bytes);
                    let output_hash = hash.finalize();
                    info!(?output_hash, "🔒 Lockfile hash before: ");
                } else {
                    let output_hash = hash.finalize();
                    info!(
                        ?output_hash,
                        "🔒 Lockfile not found. Showing default generated hash: "
                    );
                }

                if is_workspace {
                    info!("ℹ️ This manifest is in WORKSPACE configuration.");
                    let workspace_has_deps =
                        workspace_has_dependencies(custom_root, full_manifest_path)?;
                    global_has_deps = workspace_has_deps || global_has_deps;
                    if !workspace_has_deps {
                        warn!(
                            "⚠️ This extra WORKSPACE MANIFEST does not seem to contain workspace dependencies and dev-dependencies. Please check member dependencies."
                        );
                    }
                } else if !has_deps {
                    info!("😄 This extra manifest does not seem to have any dependencies.");
                    info!(
                        "🙂 If you think this is a BUG 🐞, please open an issue at <https://github.com/openSUSE-Rust/obs-service-cargo/issues>."
                    );
                }

                global_has_deps = has_deps || global_has_deps;

                if registry.update {
                    info!(
                        ?full_manifest_path,
                        "⏫ Updating dependencies for extra manifest path..."
                    );
                }

                cargo_update(
                    registry.update,
                    &registry.update_crate,
                    full_manifest_path_parent,
                    &full_manifest_path.to_string_lossy(),
                    registry.respect_lockfile,
                )?;

                if registry.update {
                    info!(
                        ?full_manifest_path,
                        "✅ Updated dependencies for extra manifest path."
                    );
                }

                if !registry.update {
                    warn!(
                        "😥 Disabled update of dependencies. You should enable this for security updates."
                    );
                }

                info!(
                    ?full_manifest_path,
                    "🚝 Attempting to fetch dependencies at extra manifest path..."
                );

                cargo_fetch(
                    full_manifest_path_parent,
                    &full_manifest_path.to_string_lossy(),
                    registry.respect_lockfile,
                )?;

                info!(
                    ?full_manifest_path,
                    "💼 Fetched dependencies for extra manifest path."
                );
            } else {
                let err = io::Error::new(io::ErrorKind::NotFound, "Path to manifest is not a file");
                error!(?err);
                return Err(err);
            }

            let possible_lockfile = full_manifest_path_parent.join("Cargo.lock");

            if possible_lockfile.is_file() {
                let lockfile_bytes = fs::read(&possible_lockfile)?;
                hash.update(&lockfile_bytes);
                let output_hash = hash.finalize();
                info!(?output_hash, "🔒 Lockfile hash after: ");
                info!(
                    ?possible_lockfile,
                    "🔒 👀 Found an extra lockfile. Adding it to home registry for vendoring."
                );
                let stripped_lockfile_path = &possible_lockfile
                    .strip_prefix(setup_workdir)
                    .unwrap_or(&possible_lockfile);
                let new_lockfile_path = &home_registry.join(stripped_lockfile_path);
                let new_lockfile_parent = new_lockfile_path.parent().unwrap_or(home_registry);
                fs::create_dir_all(new_lockfile_parent)?;
                fs::copy(&possible_lockfile, new_lockfile_path)?;
                info!(
                    ?possible_lockfile,
                    "🔒 🌟 Successfully added extra lockfile."
                );
                lockfiles.push(possible_lockfile.to_path_buf());
            }
        }

        if !no_root_manifest {
            let possible_root_lockfile = &custom_root.join("Cargo.lock");
            let possible_root_lockfile = &possible_root_lockfile
                .canonicalize()
                .unwrap_or(possible_root_lockfile.to_path_buf());
            if possible_root_lockfile.exists() {
                info!(
                    ?possible_root_lockfile,
                    "🔒 👀 Found the root lockfile. Adding it to home registry for vendoring."
                );
                let stripped_lockfile_path = possible_root_lockfile
                    .strip_prefix(setup_workdir)
                    .unwrap_or(possible_root_lockfile);
                let new_lockfile_path = &home_registry.join(stripped_lockfile_path);
                let new_lockfile_parent = new_lockfile_path.parent().unwrap_or(home_registry);
                fs::create_dir_all(new_lockfile_parent)?;
                fs::copy(possible_root_lockfile, new_lockfile_path)?;
                info!(
                    ?possible_root_lockfile,
                    "🔒 🌟 Successfully added the root lockfile."
                );
            }
            lockfiles.push(possible_root_lockfile.to_path_buf());
        }

        info!("🛡️🫥 Auditing lockfiles...");

        if let Ok(audit_result) =
            audit::perform_cargo_audit(&lockfiles, &registry.i_accept_the_risk)
        {
            audit::process_reports(audit_result).map_err(|err| {
                error!(?err);
                io::Error::new(io::ErrorKind::Interrupted, err.to_string())
            })?;
        }

        info!("🛡️🙂 All lockfiles are audited");
        info!("👉🏻🗑️ Removing unneeded directories");

        if !global_has_deps {
            info!("😄 This manifest does not seem to have any dependencies.");
            info!(
                "🙂 If you think this is a BUG 🐞, please open an issue at <https://github.com/openSUSE-Rust/obs-service-cargo/issues>."
            );
            info!(
                "🎉 No dependencies! Still, we need to regenerate the lockfile to ensure cargo works."
            );
        }

        let registry_src_dir = &home_registry_dot_cargo.join("registry").join("src");
        let registry_bin_dir = &home_registry_dot_cargo.join("bin");
        let registry_caches = [".global-cache", ".package-cache", ".package-cache-mutate"];

        if registry_src_dir.exists() {
            info!("🚮 Removing {}", registry_src_dir.display());
            fs::remove_dir_all(registry_src_dir)?;
            info!("🤯 Removed {}", registry_src_dir.display());
        }

        if registry_bin_dir.exists() {
            info!("🚮 Removing {}", registry_bin_dir.display());
            fs::remove_dir_all(registry_bin_dir)?;
            info!("🤯 Removed {}", registry_bin_dir.display());
        }

        for ca in registry_caches {
            let cache = &home_registry_dot_cargo.join(ca);
            if cache.exists() {
                info!("🚮 Removing {}", cache.display());
                fs::remove_file(cache)?;
                info!("🤯 Removed {}", cache.display());
            }
        }

        let outfile = match &registry.tag {
            Some(v) => format!("registry-{v}"),
            None => "registry".to_string(),
        };

        let mut outfile = PathBuf::from(outfile);

        let extension = match &registry.compression {
            Compression::Gz => "tar.gz",
            Compression::Xz => "tar.xz",
            Compression::Zst | Compression::Zstd => "tar.zst",
            Compression::Bz2 => "tar.bz",
            Compression::Not => "tar",
        };

        if !outfile.set_extension(extension) {
            return Err(io::Error::other("Unable to set extension"));
        }

        let roast_args = RoastArgs {
            silent: false,
            target: Some(home_registry.to_path_buf()),
            include: None,
            exclude: None,
            additional_paths: None,
            outfile: Some(outfile),
            outdir: Some(registry.outdir.to_path_buf()),
            preserve_root: false,
            reproducible: true,
            ignore_git: false,
            ignore_hidden: false,
            subcommands: None,
        };

        roast_opts(&roast_args, false)
    };

    res.inspect(|val| {
        trace!(?val);
        info!("📦 Cargo Vendor Home Registry finished.");
    })
}
