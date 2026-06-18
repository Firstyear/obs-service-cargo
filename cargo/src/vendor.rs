use libroast::common::Compression;
use libroast::operations::cli::RoastArgs;
use libroast::operations::roast::roast_opts;
use libroast::utils;
use std::fs;
use std::io;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

#[allow(unused_imports)]
use tracing::{debug, error, info, trace, warn};

use crate::cargo_commands::cargo_vendor;
use crate::cli::Opts;

pub fn run_cargo_vendor(
    setup_workdir: &Path,
    custom_root: &Path,
    vendor_opts: &Opts,
) -> io::Result<()> {
    debug!(?vendor_opts);
    if vendor_opts.no_root_manifest.is_some() {
        warn!("🛑 The `--no-root-manifest` flag is only used if `--method` is set to `registry`.");
        warn!("Ignoring `--no-root-manifest` flag...");
    }
    info!("📦 Starting Cargo Vendor");
    let tmpdir_for_config = tempfile::Builder::new()
        .prefix(".vendor_out")
        .rand_bytes(12)
        .tempdir()?;
    let to_vendor_cargo_config_dir = tmpdir_for_config.path();
    // Let's attempt a clean environment here too.
    let tempdir_for_home_registry_binding = tempfile::Builder::new()
        .prefix(".cargo")
        .rand_bytes(12)
        .tempdir()?;

    let home_registry = tempdir_for_home_registry_binding.path();
    let home_registry_dot_cargo = home_registry.join(".cargo");

    unsafe {
        std::env::set_var("CARGO_HOME", &home_registry_dot_cargo);
    }
    // Cargo vendor stdouts the configuration for config.toml
    let vendor_specific_args = vendor_opts
        .vendor_specific_args
        .as_ref()
        .unwrap_or_default();
    let res = {
        if let Some((lockfile, cargo_config_output, global_has_deps)) = cargo_vendor(
            custom_root,
            vendor_specific_args.versioned_dirs,
            vendor_specific_args.filter,
            &vendor_opts.manifest_path,
            &vendor_opts.i_accept_the_risk,
            vendor_opts.update,
            &vendor_opts.update_crate,
            vendor_opts.respect_lockfile,
        )? {
            let lockfile_parent = lockfile.parent().unwrap_or(setup_workdir);
            let lockfile_parent_stripped = lockfile_parent
                .strip_prefix(setup_workdir)
                .unwrap_or(setup_workdir);
            // NOTE: Both lockfile and dot cargo should have the same parent path.
            let target_archive_path_for_lockfile = &to_vendor_cargo_config_dir
                .join(lockfile_parent_stripped)
                .join("Cargo.lock");
            let target_archive_path_for_dot_cargo = &to_vendor_cargo_config_dir
                .join(lockfile_parent_stripped)
                .join(".cargo");
            // NOTE: It's always in the same directory as Cargo.lock.
            let path_to_vendor_dir = lockfile_parent.join("vendor");
            if !path_to_vendor_dir.is_dir() && global_has_deps {
                let msg = "🫠 Vendor directory not found... Aborting process. Please report a bug to <https://github.com/openSUSE-Rust/obs-service-cargo/issues>.";
                error!(msg);
                return Err(io::Error::new(io::ErrorKind::NotFound, msg));
            }
            let target_archive_path_for_vendor_dir = &to_vendor_cargo_config_dir
                .join(lockfile_parent_stripped)
                .join("vendor");
            fs::create_dir_all(target_archive_path_for_dot_cargo)?;
            fs::copy(lockfile, target_archive_path_for_lockfile)?;
            if !global_has_deps {
                info!("🎉 Project has no dependencies.");
            } else {
                utils::copy_dir_all(path_to_vendor_dir, target_archive_path_for_vendor_dir)?;
            }

            // NOTE maybe in the future, we might need to respect import
            // an existing `cargo.toml` but I doubt that's necessary?

            let path_to_dot_cargo_cargo_config =
                &target_archive_path_for_dot_cargo.join("config.toml");

            let mut cargo_config_file = fs::File::create(path_to_dot_cargo_cargo_config)?;

            cargo_config_file.write_all(b"# BEGIN VENDOR OPTIONS\n")?;
            cargo_config_file.write_all(cargo_config_output.as_bytes())?;
            cargo_config_file.write_all(b"\n# END VENDOR OPTIONS\n")?;

            debug!(?cargo_config_file);
        }
        let outfile = match &vendor_opts.tag {
            Some(v) => format!("vendor-{v}"),
            None => "vendor".to_string(),
        };
        let mut outfile = PathBuf::from(outfile);
        let extension = match &vendor_opts.compression {
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
            target: Some(PathBuf::from(&to_vendor_cargo_config_dir)),
            include: None,
            exclude: None,
            additional_paths: None,
            outfile: Some(outfile),
            outdir: Some(vendor_opts.outdir.to_path_buf()),
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
        info!("📦 Cargo Vendor finished.");
    })
}
