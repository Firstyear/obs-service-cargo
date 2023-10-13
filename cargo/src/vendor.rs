// SPDX-License-Identifier: MPL-2.0

// Copyright (C) 2023  Soc Virnyl Estela

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use crate::cli::Compression;
use crate::utils::cargo_command;
use crate::utils::compress;

#[allow(unused_imports)]
use tracing::{debug, error, info, trace, warn, Level};

pub fn update(prjdir: impl AsRef<Path>, manifest_path: impl AsRef<Path>) -> io::Result<()> {
    info!("⏫ Updating dependencies before vendor");
    let update_options: Vec<OsString> = vec![
        "-vv".into(),
        "--manifest-path".into(),
        manifest_path.as_ref().into(),
    ];

    cargo_command("update", &update_options, &prjdir).map_err(|e| {
        error!(err = %e);
        io::Error::new(io::ErrorKind::Other, "Unable to execute cargo")
    })?;
    info!("⏫ Successfully ran cargo update");
    Ok(())
}

pub fn vendor(
    prjdir: impl AsRef<Path>,
    cargo_config: impl AsRef<Path>,
    manifest_path: impl AsRef<Path>,
    extra_manifest_paths: &[impl AsRef<Path>],
) -> io::Result<()> {
    let mut vendor_options: Vec<OsString> = vec![
        "-vv".into(),
        "--manifest-path".into(),
        manifest_path.as_ref().into(),
    ];

    for ex_path in extra_manifest_paths {
        vendor_options.push("--sync".into());
        vendor_options.push(ex_path.as_ref().into());
    }

    debug!(?vendor_options);

    let cargo_vendor_output = cargo_command("vendor", &vendor_options, &prjdir).map_err(|e| {
        error!(err = %e);
        io::Error::new(io::ErrorKind::Other, "Unable to execute cargo")
    })?;

    let mut file_cargo_config = fs::File::create(cargo_config.as_ref())?;
    // Write the stdout which is used by the package later.
    file_cargo_config.write_all(cargo_vendor_output.as_bytes())?;

    Ok(())
}

pub fn compress(
    outpath: impl AsRef<Path>,
    prjdir: impl AsRef<Path>,
    paths_to_archive: &[impl AsRef<Path>],
    compression: &Compression,
) -> io::Result<()> {
    info!("📦 Archiving vendored dependencies...");

    // RATIONALE: We copy Cargo.lock by default, updated or not updated
    // `../` relative to `vendor/` directory.
    // CONSIDERATIONS:
    // Maybe in the future we can check if Cargo.toml points to a workspace
    // using the `toml` crate. For now, we aggressively just copy `../Cargo.lock`
    // relative to vendor directory if it exists. Even if it is a workspace,
    // it will still copy the project's `Cargo.lock` because we still run
    // `vendor` anyway starting at the root of the project where the lockfile resides.
    // NOTE: 1. The members in that workspace still requires that root lockfile.
    // NOTE: 2. Members of that workspace cannot generate their own lockfiles.
    // NOTE: 3. If they are not members, we slap that file into their own compressed vendored
    //          tarball

    let mut vendor_out = outpath.as_ref().join("vendor");
    match compression {
        Compression::Gz => {
            vendor_out.set_extension("tar.gz");
            if vendor_out.exists() {
                warn!(
                    replacing = ?vendor_out,
                    "🔦 Compressed tarball for vendor exists AND will be replaced."
                );
            }
            compress::targz(&vendor_out, &prjdir, &paths_to_archive)?;
            debug!("Compressed to {}", vendor_out.to_string_lossy());
        }
        Compression::Xz => {
            vendor_out.set_extension("tar.xz");
            if vendor_out.exists() {
                warn!(
                    replacing = ?vendor_out,
                    "🔦 Compressed tarball for vendor exists AND will be replaced."
                );
            }
            compress::tarxz(&vendor_out, &prjdir, &paths_to_archive)?;
            debug!("Compressed to {}", vendor_out.to_string_lossy());
        }
        Compression::Zst => {
            vendor_out.set_extension("tar.zst");
            if vendor_out.exists() {
                warn!(
                    replacing = ?vendor_out,
                    "🔦 Compressed tarball for vendor exists AND will be replaced."
                );
            }
            compress::tarzst(&vendor_out, &prjdir, &paths_to_archive)?;
            debug!("Compressed to {}", vendor_out.to_string_lossy());
        }
    };
    debug!("Finished creating {} compressed tarball", compression);

    Ok(())
}

pub fn is_workspace(src: &Path) -> io::Result<bool> {
    if let Ok(manifest) = fs::read_to_string(src) {
        if let Ok(manifest_data) = toml::from_str::<toml::Value>(&manifest) {
            if manifest_data.get("workspace").is_some() {
                return Ok(true);
            } else {
                return Ok(false);
            };
        };
    }
    return Err(io::Error::new(
        io::ErrorKind::NotFound,
        src.to_string_lossy(),
    ));
}

pub fn has_dependencies(src: &Path) -> io::Result<bool> {
    if let Ok(manifest) = fs::read_to_string(src) {
        if let Ok(manifest_data) = toml::from_str::<toml::Value>(&manifest) {
            if manifest_data.get("dependencies").is_some()
                || manifest_data.get("dev-dependencies").is_some()
            {
                return Ok(true);
            } else {
                return Ok(false);
            };
        };
    }
    return Err(io::Error::new(
        io::ErrorKind::NotFound,
        src.to_string_lossy(),
    ));
}
