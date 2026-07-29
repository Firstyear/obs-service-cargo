// SPDX-License-Identifier: MPL-2.0

// Copyright (C) 2024 To all Contributors of this project listed in CONTRIBUTORS.md

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::io;
use std::path::{Path, PathBuf};

use crate::consts::VENDOR_PATH_PREFIX;
use crate::registry::run_cargo_vendor_home_registry;
use crate::vendor::run_cargo_vendor;
use libroast::common::Compression;

use clap::{Args, Parser, ValueEnum};
use libroast::operations::cli::{RawArgs, RoastScmArgs};
use libroast::operations::raw::raw_opts;
use libroast::utils::copy_dir_all;
use libroast::{decompress, utils};

#[allow(unused_imports)]
use tracing::{Level, debug, error, info, trace, warn};

#[derive(Debug, Clone, ValueEnum, Default)]
pub enum Method {
    Registry,
    #[default]
    Vendor,
}

#[derive(Parser, Debug)]
#[command(
    author,
    name = "cargo_vendor",
    version,
    about = "OBS Source Service to vendor all crates.io and dependencies for Rust project locally",
    after_long_help = "Set verbosity and tracing through `RUST_LOG` environmental variable e.g. `RUST_LOG=trace`

Bugs can be reported on GitHub: https://github.com/openSUSE/obs-service-cargo_vendor/issues",
    max_term_width = 120
)]
pub struct Opts {
    #[arg(long, action = clap::ArgAction::Set, default_value_t = false, help = "Whether to generate or update a changelog file or not. To be passed to Roast SCM.")]
    pub changesgenerate: bool,
    #[arg(
        long,
        short = 'A',
        required_if_eq("changesgenerate", "true"),
        help = "Author to include during the changelog generation. To be passed to Roast SCM."
    )]
    pub changesauthor: Option<String>,
    #[arg(
        long,
        short = 'e',
        help = "Email of author to include during the changelog generation. To be passed to Roast SCM."
    )]
    pub changesemail: Option<String>,
    #[arg(
        long,
        alias = "caof",
        requires("changesauthor"),
        help = "Whether to specify a path to the changes file. Otherwise, it is the current \
                directory and the filename is the same filename prefix of the generated tarball \
                e.g. `source.tar.xz` will have `source.changes` file. If file exists, append the \
                newest changes to the top-most part of the text file. To be passed to Roast SCM."
    )]
    pub changesoutfile: Option<PathBuf>,
    #[arg(
        long,
        help = "Whether to hard code the version or not. Set it to hard code one, otherwise, it \
                will use the generated version internally. To be passed to Roast SCM."
    )]
    pub set_version: Option<String>,
    #[arg(
        long,
        help = "Whether to hard code the name or not. Set it to hard code one, otherwise, it will \
                use the generated name internally. To be passed to Roast SCM."
    )]
    pub set_name: Option<String>,
    #[arg(
        long,
        short = 'E',
        help = "Additional paths such as files or directories from within target repository's \
                work directory to exclude when generating the archive. To be passed to Roast SCM."
    )]
    pub exclude: Option<Vec<PathBuf>>,
    #[arg(
        long,
        help = "Revision or tag. It can also be a specific commit hash or branch. Supports <https://git-scm.com/docs/git-rev-parse.html#_specifying_revisions>."
    )]
    pub revision: Option<String>,
    #[arg(
        long,
        help = "Pass a regex with capture groups. Required by `versionrewritepattern` flag. Each \
                capture group is labelled through increments of 1. To be passed to Roast SCM.",
        requires = "versionrewritepattern"
    )]
    pub versionrewriteregex: Option<String>,
    #[arg(
        long,
        help = "Pass a pattern from the capture groups from `versionrewriteregex` flag. To be passed to Roast SCM."
    )]
    pub versionrewritepattern: Option<String>,
    #[arg(
        long,
        value_enum,
        default_value_t,
        help = "Whether to use vendor or the registry. To be passed to Roast SCM."
    )]
    pub method: Method,
    #[arg(
        long,
        visible_aliases = ["srctar", "srcdir", "target", "url"],
        help = "Where to find sources. Source is either a directory or a source tarball or a URL to a remote git repository."
    )]
    pub src: String,
    #[arg(
        long,
        short = 'C',
        help = "Whether you want to manually set the root of the project. Useful with a combination with `--manifest-path` (aliased as `--cargotoml`) or `--no-root-manifest`."
    )]
    pub custom_root: Option<String>,
    #[arg(
        long,
        short = 'N',
        help = "Available only if `--method` is set to registry. If a project has no root manifest, this flag is useful for those situations to set the manifest path manually. Useful in combination with `--manifest-path` (aliased as `--cargotoml`) flag."
    )]
    pub no_root_manifest: Option<bool>,
    #[arg(
        long,
        value_enum,
        default_value_t,
        help = "What compression algorithm to use. Set to `not` if you just want a normal tarball with no compression."
    )]
    pub compression: Compression,
    #[arg(
        long,
        help = "Tag some files for multi-vendor and multi-cargo_config projects"
    )]
    pub tag: Option<String>,
    #[arg(long, help = "Other cargo manifest files to sync with vendor or registry. Behaviour between methods changes. Consult the documentation.", visible_aliases = ["cargotoml"])]
    pub manifest_path: Vec<PathBuf>,
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set, help = "Update dependencies or not.")]
    pub update: bool,

    #[arg(
        long,
        help = "Path to a config.toml to used to override options during vendor."
    )]
    pub import_cargo_config: Option<PathBuf>,

    #[arg(
        long,
        help = "Where to output vendor.tar* and cargo_config if method is vendor and registry.tar* if method is registry. If using with `osc service`, this option is automatically appended."
    )]
    pub outdir: PathBuf,
    #[arg(
        long,
        default_value = "auto",
        default_missing_value = "always",
        value_name = "WHEN",
        help = "Whether WHEN to color output or not"
    )]
    pub color: clap::ColorChoice,
    #[arg(
        long,
        short = 'L',
        action = clap::ArgAction::Set,
        help = "Whether to respect Cargo.lock or lockfiles by passing the `--locked` flag.",
        default_value_t = false
    )]
    pub respect_lockfile: bool,
    #[arg(
        long,
        help = "A list of rustsec-id's to ignore. By setting this value, you acknowledge that this issue does not affect your package and you should be exempt from resolving it."
    )]
    pub i_accept_the_risk: Vec<String>,
    #[arg(
        long,
        help = "Set of specific crates to update. If not empty, it will set the global update flag to false. You can specify a valid version string by adding a `@` after the crate name e.g. `foo@1.2.3`. You can also do recursive updates of a crate by appending `recursive` to `@` e.g. `foo@recursive`. However, recursive can't be used with precise. You can specify a manifest path to update a package with `+` e.g. `foo@1.0+foo/better/Cargo.toml`. See `cargo help update` for info about how to update specific crates."
    )]
    pub update_crate: Vec<String>,
    #[clap(flatten)]
    pub vendor_specific_args: Option<VendorArgs>,
}

#[derive(Debug, Args, Clone)]
pub struct VendorArgs {
    #[arg(
        long,
        action = clap::ArgAction::Set,
        default_value_t = false,
        help = "Available only if `--method` is set to vendor. EXPERIMENTAL: Reduce vendor-tarball size by filtering out non-Linux dependencies."
    )]
    pub filter: bool,
    #[arg(
        long,
        action = clap::ArgAction::Set,
        default_value_t = true,
        help = "Available only if `--method` is set to vendor. Whether to use the `--versioned-dirs` flag of cargo-vendor."
    )]
    pub versioned_dirs: bool,
}

impl Default for VendorArgs {
    fn default() -> Self {
        VendorArgs {
            filter: false,
            versioned_dirs: true,
        }
    }
}

impl Default for &VendorArgs {
    fn default() -> Self {
        static VENDOR_ARGS: VendorArgs = VendorArgs {
            filter: false,
            versioned_dirs: true,
        };
        &VENDOR_ARGS
    }
}

pub fn decompress(comp_type: &Compression, outdir: &Path, src: &Path) -> io::Result<()> {
    match comp_type {
        Compression::Gz => decompress::targz(outdir, src),
        Compression::Xz => decompress::tarxz(outdir, src),
        Compression::Zst | Compression::Zstd => decompress::tarzst(outdir, src),
        Compression::Bz2 => decompress::tarbz2(outdir, src),
        Compression::Not => decompress::vanilla(outdir, src),
    }
}

impl Opts {
    pub fn run_vendor(&mut self) -> io::Result<()> {
        debug!(?self);
        let is_url = url::Url::parse(&self.src).is_ok();
        let tempdir_for_workdir = tempfile::Builder::new()
            .prefix(VENDOR_PATH_PREFIX)
            .rand_bytes(12)
            .tempdir()?;
        let workdir = tempdir_for_workdir.path().to_path_buf();
        debug!(?workdir);
        let target = if let Ok(result) = utils::process_globs(std::path::Path::new(&self.src)) {
            result
        } else {
            if is_url {
                warn!(
                    "⚠️ Not a glob input. Please check if you are passing a path to a file or a directory. We expect that you are passing a URL to a remote git repository."
                );
                warn!(
                    "⚠️ Ensure you pass a commit hash or version to `--revision`. Otherwise, this will fail."
                );
                warn!(
                    " ⚠️ This is an experimental feature. Please file a bug report at <https://github.com/openSUSE-Rust/obs-service-cargo/issues/new/choose>. Thank you!"
                );
            }
            std::path::Path::new(&self.src).to_path_buf()
        };

        if target.is_dir() {
            copy_dir_all(&target, &workdir)?;
        } else if target.is_file() && utils::is_supported_format(&target).is_ok() {
            let raw_args = RawArgs {
                target: Some(target.to_path_buf()),
                outdir: Some(workdir.to_path_buf()),
                silent: false,
                subcommands: None,
            };
            raw_opts(raw_args, false)?;
        } else if is_url {
            if let Some(revision) = &self.revision {
                let roast_scm_args = RoastScmArgs {
                    silent: false,
                    changesgenerate: self.changesgenerate,
                    changesauthor: self.changesauthor.clone(),
                    changesemail: self.changesemail.clone(),
                    changesoutfile: self.changesoutfile.clone(),
                    set_version: self.set_version.clone(),
                    set_name: self.set_name.clone(),
                    git_repository_url: Some(self.src.to_string()),
                    exclude: None,
                    revision: Some(revision.to_string()),
                    versionrewriteregex: self.versionrewriteregex.clone(),
                    versionrewritepattern: self.versionrewritepattern.clone(),
                    depth: 0,
                    is_temporary: true,
                    outfile: None,
                    outdir: Some(self.outdir.to_path_buf()),
                    reproducible: true,
                    ignore_git: true,
                    ignore_hidden: false,
                    compression: self.compression,
                    subcommands: None,
                };

                libroast::operations::roast_scm::roast_scm_opts(
                    Some(workdir.clone()),
                    &roast_scm_args,
                    false,
                )?;
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Revision is empty.",
                ));
            }
        } else {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Unsupported format found",
            ));
        }

        let setup_workdir = {
            let dirs: Vec<Result<std::fs::DirEntry, std::io::Error>> =
                std::fs::read_dir(&workdir)?.collect();
            debug!(?dirs, "List of files and directories of the workdir");
            if dirs.len() > 1 {
                debug!(?workdir);
                workdir.to_path_buf()
            } else {
                match dirs.into_iter().next_back() {
                    Some(p) => match p {
                        Ok(dir) => {
                            if dir.path().is_dir() {
                                debug!("{}", dir.path().display());
                                // NOTE: return new workdir
                                dir.path()
                            } else {
                                error!(
                                    ?dir,
                                    "Tarball was extracted but got a file and not a possible top-level directory."
                                );
                                return Err(io::Error::new(
                                    io::ErrorKind::Interrupted,
                                    "No top-level directory found after tarball was extracted"
                                        .to_string(),
                                ));
                            }
                        }
                        Err(err) => {
                            error!(?err, "Failed to read directory entry");
                            return Err(err);
                        }
                    },
                    None => {
                        error!("This should be unreachable here");
                        unreachable!();
                    }
                }
            }
        };

        let custom_root = if let Some(custom_root) = &self.custom_root {
            info!(?custom_root, "ℹ️  Custom root is set.");
            setup_workdir.join(custom_root)
        } else {
            setup_workdir.to_path_buf()
        };

        // It won't make sense for update to be globally true while specifying to update a package
        if !&self.update_crate.is_empty() {
            warn!(?self.update_crate,
                "⚠️ Global update flag was set to false because specific crates to update are set!"
            );
            self.update = false;
        }

        // If a config.toml exists, and we want to use it, import it now.
        if let Some(cargo_config) = self.import_cargo_config.as_ref() {
            let cargo_config_dir = custom_root.join(".cargo");
            if !cargo_config_dir.exists() {
                std::fs::create_dir(&cargo_config_dir).inspect_err(|_| {
                    error!(
                        "Unable to create .cargo directory {}",
                        cargo_config_dir.display()
                    );
                })?;
            }

            let cargo_config_path = cargo_config_dir.join("config.toml");
            std::fs::copy(cargo_config, &cargo_config_path).inspect_err(|_| {
                error!(
                    "Unable to import cargo config from {}",
                    cargo_config.display()
                );
            })?;
        }

        if setup_workdir.exists() && setup_workdir.is_dir() {
            match &self.method {
                Method::Registry => {
                    run_cargo_vendor_home_registry(&setup_workdir, &custom_root, self)
                }
                Method::Vendor => run_cargo_vendor(&setup_workdir, &custom_root, self),
            }.inspect_err(|err| {
                match err.kind() {
                    io::ErrorKind::StorageFull => {
                        let dir = std::env::temp_dir();
			error!(?err);
			error!(
r#"🛑 Your `$TMPDIR` at {} has less storage space.
Ensure that your `$TMPDIR` at {} has a large storage space than the vendor, registry, and extracted or copied source code.
ℹ️ A workaround is setting `$TMPDIR` to another directory larger than the total size of your vendored tarball. For example,
```
export TMPDIR="$HOME/.cache"
osc service -vvv mr cargo_vendor
```
"#, dir.display(), dir.display());
                    }
                    _ => error!(?err)
                }
            })?;
        } else {
            let mut msg: String =
                "It seems that the setup workdir is not a directory or does not exist.".to_string();
            if self.custom_root.is_some() {
                msg.push_str(" Please check if your custom root has been setup properly!");
            } else {
                msg.push_str(" This seems to be a bug. Please file an issue at <https://github.com/openSUSE-Rust/obs-service-cargo/issues>.");
            }
            error!(msg);
            return Err(io::Error::other(msg));
        }
        info!("🌟 OBS Service Cargo finished.");
        tempdir_for_workdir.close()?;
        Ok(())
    }
}
