#![allow(clippy::unwrap_used)]
#![allow(unused_imports)]

use blake3::Hasher;
use libroast::{
    common::Compression,
    operations::{cli::RawArgs, raw::raw_opts},
};
use obs_service_cargo::cli::{self, Method, VendorArgs};
use rand::prelude::*;
use std::{io, path::PathBuf};
use test_log::test;
use tokio::fs;
use tokio_test::task::spawn;
use tracing::info;

const MANIFEST_DIR: &str = std::env!("CARGO_MANIFEST_DIR", "No such manifest dir");

async fn another_vendor_helper(source: &str, update: bool) -> io::Result<PathBuf> {
    let mut rng = rand::rng();
    let random_tag: u8 = rng.random();
    let random_tag = random_tag.to_string();
    let response = reqwest::get(source).await.unwrap();
    let fname = response
        .url()
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .and_then(|name| if name.is_empty() { None } else { Some(name) })
        .unwrap_or("balls");
    info!("Source file: {}", &fname);
    let outfile = format!("/{}/{}", "tmp", &fname);
    info!("Downloaded to: '{:?}'", &outfile);
    fs::File::create(&outfile).await.unwrap();
    let outfile = PathBuf::from(&outfile);
    let data = response.bytes().await.unwrap();
    let data = data.to_vec();
    fs::write(&outfile, data).await.unwrap();
    let outdir = PathBuf::from("/tmp");
    let vendor_specific_args = VendorArgs {
        filter: false,
        versioned_dirs: true,
    };
    let mut opt = cli::Opts {
        changesgenerate: false,
        changesauthor: None,
        changesemail: None,
        changesoutfile: None,
        set_version: None,
        set_name: None,
        exclude: None,
        revision: None,
        versionrewriteregex: None,
        versionrewritepattern: None,
        method: Method::Vendor,
        src: outfile.to_string_lossy().to_string(),
        custom_root: None,
        no_root_manifest: Some(false),
        compression: Compression::default(),
        tag: Some(random_tag.clone()),
        manifest_path: vec![],
        update,
        import_cargo_config: None,
        outdir: outdir.to_path_buf(),
        color: clap::ColorChoice::Auto,
        respect_lockfile: false,
        i_accept_the_risk: vec![],
        update_crate: vec![],
        vendor_specific_args: Some(vendor_specific_args),
    };

    let res = opt.run_vendor();
    assert!(res.is_ok());
    let vendor_tarball = match opt.method {
        Method::Registry => format!("registry-{}.tar.zst", &random_tag),
        Method::Vendor => format!("vendor-{}.tar.zst", &random_tag),
    };
    let vendor_tarball_path = &outdir.join(vendor_tarball);

    let raw_outdir = PathBuf::from("/tmp").join(random_tag).join("output");
    let raw_args = RawArgs {
        silent: false,
        target: Some(vendor_tarball_path.to_path_buf()),
        outdir: Some(raw_outdir.clone()),
        subcommands: None,
    };
    raw_opts(raw_args, false)?;
    let cargo_lockfile_path = raw_outdir.join("Cargo.lock");
    assert!(cargo_lockfile_path.is_file());
    Ok(cargo_lockfile_path)
}

#[test(tokio::test)]
async fn lockfile_does_not_change_if_update_is_false() -> io::Result<()> {
    let source =
        "https://github.com/openSUSE-Rust/obs-service-cargo/archive/refs/tags/v4.3.6.tar.gz";
    let first = another_vendor_helper(source, false).await?;
    let second = another_vendor_helper(source, false).await?;
    let mut hasher1 = Hasher::default();
    let mut hasher2 = Hasher::default();
    let first_bytes = fs::read(&first).await?;
    let second_bytes = fs::read(&second).await?;
    hasher1.update(&first_bytes);
    hasher2.update(&second_bytes);

    assert!(hasher1.finalize() == hasher2.finalize());
    Ok(())
}

#[test(tokio::test)]
async fn lockfile_does_change_if_update_is_true() -> io::Result<()> {
    let source =
        "https://github.com/openSUSE-Rust/obs-service-cargo/archive/refs/tags/v4.3.6.tar.gz";
    let first = another_vendor_helper(source, false).await?;
    let second = another_vendor_helper(source, true).await?;
    let mut hasher1 = Hasher::default();
    let mut hasher2 = Hasher::default();
    let first_bytes = fs::read(&first).await?;
    let second_bytes = fs::read(&second).await?;
    hasher1.update(&first_bytes);
    hasher2.update(&second_bytes);

    assert!(hasher1.finalize() != hasher2.finalize());
    Ok(())
}

async fn vendor_source(source: &str, filter: bool) -> io::Result<PathBuf> {
    let mut rng = rand::rng();
    let random_tag: u8 = rng.random();
    let random_tag = random_tag.to_string();
    let response = reqwest::get(source).await.unwrap();
    let fname = response
        .url()
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .and_then(|name| if name.is_empty() { None } else { Some(name) })
        .unwrap_or("balls");
    info!("Source file: {}", &fname);
    let outfile = format!("/{}/{}", "tmp", &fname);
    info!("Downloaded to: '{:?}'", &outfile);
    fs::File::create(&outfile).await.unwrap();
    let outfile = PathBuf::from(&outfile);
    let data = response.bytes().await.unwrap();
    let data = data.to_vec();
    fs::write(&outfile, data).await.unwrap();
    let outdir = PathBuf::from("/tmp");
    let vendor_specific_args = VendorArgs {
        filter,
        ..VendorArgs::default()
    };
    let mut opt = cli::Opts {
        changesgenerate: false,
        changesauthor: None,
        changesemail: None,
        changesoutfile: None,
        set_version: None,
        set_name: None,
        exclude: None,
        revision: None,
        versionrewriteregex: None,
        versionrewritepattern: None,
        method: Method::Vendor,
        src: outfile.to_string_lossy().to_string(),
        custom_root: None,
        no_root_manifest: None,
        compression: Compression::default(),
        tag: Some(random_tag.clone()),
        manifest_path: vec![],
        update: true,
        import_cargo_config: None,
        outdir: outdir.to_path_buf(),
        color: clap::ColorChoice::Auto,
        respect_lockfile: false,
        i_accept_the_risk: vec![],
        update_crate: vec![],
        vendor_specific_args: Some(vendor_specific_args),
    };

    let res = opt.run_vendor();
    assert!(res.is_ok());
    let vendor_tarball = match opt.method {
        Method::Registry => format!("registry-{}.tar.zst", &random_tag),
        Method::Vendor => format!("vendor-{}.tar.zst", &random_tag),
    };
    let vendor_tarball_path = &outdir.join(vendor_tarball);

    let raw_outdir = PathBuf::from("/tmp").join(random_tag).join("output");
    let raw_args = RawArgs {
        silent: false,
        target: Some(vendor_tarball_path.to_path_buf()),
        outdir: Some(raw_outdir.clone()),
        subcommands: None,
    };
    assert!(raw_opts(raw_args, false).is_ok());
    let vendor_path = raw_outdir.join("vendor");
    let cargo_config_path = raw_outdir.join(".cargo").join("config.toml");
    let cargo_lock_path = raw_outdir.join("Cargo.lock");
    assert!(vendor_tarball_path.is_file());
    if *source == *"https://github.com/elliot40404/bonk/archive/refs/tags/v0.4.0.tar.gz" {
        assert!(!vendor_path.is_dir());
    } else {
        assert!(vendor_path.is_dir());
    }
    assert!(cargo_config_path.is_file());
    assert!(cargo_lock_path.is_file());
    Ok(outfile)
}

#[test(tokio::test)]
async fn no_filter_vendor_sources() -> io::Result<()> {
    let sources = [
        // NOTE: This should not vendor anything as it does not contain any dependencies
        "https://github.com/elliot40404/bonk/archive/refs/tags/v0.4.0.tar.gz",
        // NOTE: This should vendor
        "https://codeberg.org/Rusty-Geckos/roast/archive/tags/v5.1.7.tar.gz",
        // NOTE: This should not stack overflow
        "https://github.com/casey/just/archive/refs/tags/1.38.0.tar.gz",
    ];

    spawn(async move {
        for src in sources {
            vendor_source(src, false).await.unwrap();
        }
    })
    .await;

    Ok(())
}

#[test(tokio::test)]
async fn filter_vendor_sources() -> io::Result<()> {
    let sources = [
        // NOTE: This should not vendor anything as it does not contain any dependencies
        "https://github.com/elliot40404/bonk/archive/refs/tags/v0.4.0.tar.gz",
        // NOTE: This should vendor
        "https://codeberg.org/Rusty-Geckos/roast/archive/tags/v5.1.7.tar.gz",
        // NOTE: This should not stack overflow
        "https://github.com/casey/just/archive/refs/tags/1.38.0.tar.gz",
    ];

    spawn(async move {
        for src in sources {
            vendor_source(src, false).await.unwrap();
        }
    })
    .await;

    Ok(())
}

#[test(tokio::test)]
async fn vendor_registry_test_with_no_root_manifest() -> io::Result<()> {
    let source = "https://github.com/ibm-s390-linux/s390-tools/archive/refs/tags/v2.29.0.tar.gz";
    let mut rng = rand::rng();
    let random_tag: u8 = rng.random();
    let random_tag = random_tag.to_string();
    let response = reqwest::get(source).await.unwrap();
    let fname = response
        .url()
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .and_then(|name| if name.is_empty() { None } else { Some(name) })
        .unwrap_or("balls");
    info!("Source file: {}", &fname);
    let outfile = format!("/{}/{}", "tmp", &fname);
    info!("Downloaded to: '{:?}'", &outfile);
    fs::File::create(&outfile).await.unwrap();
    let outfile = PathBuf::from(&outfile);
    let data = response.bytes().await.unwrap();
    let data = data.to_vec();
    fs::write(&outfile, data).await.unwrap();
    let outdir = PathBuf::from("/tmp");
    let vendor_specific_args = VendorArgs {
        filter: true,
        versioned_dirs: true,
    };
    let mut opt = cli::Opts {
        changesgenerate: false,
        changesauthor: None,
        changesemail: None,
        changesoutfile: None,
        set_version: None,
        set_name: None,
        exclude: None,
        revision: None,
        versionrewriteregex: None,
        versionrewritepattern: None,
        custom_root: None,
        update_crate: vec![],
        no_root_manifest: Some(true),
        method: Method::Registry,
        src: outfile.to_string_lossy().to_string(),
        compression: Compression::default(),
        tag: Some(random_tag.clone()),
        manifest_path: [PathBuf::from("rust/pvsecret/Cargo.toml")].to_vec(),
        update: true,
        import_cargo_config: None,
        vendor_specific_args: Some(vendor_specific_args),
        respect_lockfile: false,
        outdir: outdir.to_path_buf(),
        color: clap::ColorChoice::Auto,
        i_accept_the_risk: vec![],
    };

    let res = opt.run_vendor();
    assert!(res.is_ok());
    let vendor_tarball = match opt.method {
        Method::Registry => format!("registry-{}.tar.zst", &random_tag),
        Method::Vendor => format!("vendor-{}.tar.zst", &random_tag),
    };

    let vendor_tarball_path = &outdir.join(vendor_tarball);
    assert!(vendor_tarball_path.is_file());

    let raw_outdir = PathBuf::from("/tmp").join(random_tag).join("output");
    let raw_args = RawArgs {
        silent: false,
        target: Some(vendor_tarball_path.to_path_buf()),
        outdir: Some(raw_outdir.clone()),
        subcommands: None,
    };
    raw_opts(raw_args, false)?;
    let vendor_path = raw_outdir.join("rust").join("pvsecret").join("vendor");
    let cargo_config_path = raw_outdir
        .join("rust")
        .join("pvsecret")
        .join(".cargo")
        .join("config.toml");
    // NOTE: This should always stay at the top-most level
    // Since
    // 1. It's not affected by where the custom root is
    // 2. It does not make sense to put it anywhere but top-most level directory
    let cargo_registry_path = raw_outdir.join(".cargo").join("registry");
    let cargo_possible_lockfile_path = raw_outdir.join("rust").join("pvsecret").join("Cargo.lock");
    assert!(!vendor_path.is_dir());
    assert!(!cargo_config_path.is_file());
    assert!(cargo_registry_path.is_dir());
    assert!(cargo_possible_lockfile_path.is_file());
    Ok(())
}

#[test(tokio::test)]
async fn manifest_paths_with_vendor() -> io::Result<()> {
    let source = "https://github.com/influxdata/flux/archive/refs/tags/v0.194.4.tar.gz";
    let mut rng = rand::rng();
    let random_tag: u8 = rng.random();
    let random_tag = random_tag.to_string();
    let response = reqwest::get(source).await.unwrap();
    let fname = response
        .url()
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .and_then(|name| if name.is_empty() { None } else { Some(name) })
        .unwrap_or("balls");
    info!("Source file: {}", &fname);
    let outfile = format!("/{}/{}", "tmp", &fname);
    info!("Downloaded to: '{:?}'", &outfile);
    fs::File::create(&outfile).await.unwrap();
    let outfile = PathBuf::from(&outfile);
    let data = response.bytes().await.unwrap();
    let data = data.to_vec();
    fs::write(&outfile, data).await.unwrap();
    let outdir = PathBuf::from("/tmp");
    let mut opt = cli::Opts {
        changesgenerate: false,
        changesauthor: None,
        changesemail: None,
        changesoutfile: None,
        set_version: None,
        set_name: None,
        exclude: None,
        revision: None,
        versionrewriteregex: None,
        versionrewritepattern: None,
        update_crate: vec![],
        no_root_manifest: None,
        custom_root: None,
        method: Method::Vendor,
        src: outfile.to_string_lossy().to_string(),
        compression: Compression::default(),
        tag: Some(random_tag.clone()),
        manifest_path: [PathBuf::from("libflux/Cargo.toml")].to_vec(),
        respect_lockfile: false,
        update: true,
        import_cargo_config: None,
        outdir: outdir.to_path_buf(),
        color: clap::ColorChoice::Auto,
        i_accept_the_risk: vec![],
        vendor_specific_args: None,
    };

    let res = opt.run_vendor();
    assert!(res.is_ok());
    let vendor_tarball = match opt.method {
        Method::Registry => format!("registry-{}.tar.zst", &random_tag),
        Method::Vendor => format!("vendor-{}.tar.zst", &random_tag),
    };

    let vendor_tarball_path = &outdir.join(vendor_tarball);
    assert!(vendor_tarball_path.is_file());

    let raw_outdir = PathBuf::from("/tmp").join(random_tag).join("output");
    let raw_args = RawArgs {
        silent: false,
        target: Some(vendor_tarball_path.to_path_buf()),
        outdir: Some(raw_outdir.clone()),
        subcommands: None,
    };
    raw_opts(raw_args, false)?;
    let vendor_path = raw_outdir.join("libflux").join("vendor");
    let cargo_config_path = raw_outdir
        .join("libflux")
        .join(".cargo")
        .join("config.toml");
    let cargo_lock_path = raw_outdir.join("libflux").join("Cargo.lock");
    assert!(vendor_path.is_dir());
    assert!(cargo_config_path.is_file());
    assert!(cargo_lock_path.is_file());
    Ok(())
}

#[test(tokio::test)]
async fn custom_root_test_1() -> io::Result<()> {
    let source = "https://github.com/influxdata/flux/archive/refs/tags/v0.194.4.tar.gz";
    let mut rng = rand::rng();
    let random_tag: u8 = rng.random();
    let random_tag = random_tag.to_string();
    let response = reqwest::get(source).await.unwrap();
    let fname = response
        .url()
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .and_then(|name| if name.is_empty() { None } else { Some(name) })
        .unwrap_or("balls");
    info!("Source file: {}", &fname);
    let outfile = format!("/{}/{}", "tmp", &fname);
    info!("Downloaded to: '{:?}'", &outfile);
    fs::File::create(&outfile).await.unwrap();
    let outfile = PathBuf::from(&outfile);
    let data = response.bytes().await.unwrap();
    let data = data.to_vec();
    fs::write(&outfile, data).await.unwrap();
    let outdir = PathBuf::from("/tmp");
    let mut opt = cli::Opts {
        changesgenerate: false,
        changesauthor: None,
        changesemail: None,
        changesoutfile: None,
        set_version: None,
        set_name: None,
        exclude: None,
        revision: None,
        versionrewriteregex: None,
        versionrewritepattern: None,
        update_crate: vec![],
        no_root_manifest: None,
        respect_lockfile: false,
        custom_root: Some("libflux".to_string()),
        method: Method::Vendor,
        src: outfile.to_string_lossy().to_string(),
        compression: Compression::default(),
        tag: Some(random_tag.clone()),
        manifest_path: vec![],
        update: true,
        import_cargo_config: None,
        outdir: outdir.to_path_buf(),
        color: clap::ColorChoice::Auto,
        i_accept_the_risk: vec![],
        vendor_specific_args: None,
    };

    let res = opt.run_vendor();
    assert!(res.is_ok());
    let vendor_tarball = match opt.method {
        Method::Registry => format!("registry-{}.tar.zst", &random_tag),
        Method::Vendor => format!("vendor-{}.tar.zst", &random_tag),
    };

    let vendor_tarball_path = &outdir.join(vendor_tarball);
    assert!(vendor_tarball_path.is_file());

    let raw_outdir = PathBuf::from("/tmp").join(random_tag).join("output");
    let raw_args = RawArgs {
        silent: false,
        target: Some(vendor_tarball_path.to_path_buf()),
        outdir: Some(raw_outdir.clone()),
        subcommands: None,
    };
    raw_opts(raw_args, false)?;
    let vendor_path = raw_outdir.join("libflux").join("vendor");
    let cargo_config_path = raw_outdir
        .join("libflux")
        .join(".cargo")
        .join("config.toml");
    let cargo_lock_path = raw_outdir.join("libflux").join("Cargo.lock");
    assert!(vendor_path.is_dir());
    assert!(cargo_config_path.is_file());
    assert!(cargo_lock_path.is_file());
    Ok(())
}

#[test(tokio::test)]
async fn custom_root_test_2() -> io::Result<()> {
    let source = "https://github.com/influxdata/flux/archive/refs/tags/v0.194.4.tar.gz";
    let mut rng = rand::rng();
    let random_tag: u8 = rng.random();
    let random_tag = random_tag.to_string();
    let response = reqwest::get(source).await.unwrap();
    let fname = response
        .url()
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .and_then(|name| if name.is_empty() { None } else { Some(name) })
        .unwrap_or("balls");
    info!("Source file: {}", &fname);
    let outfile = format!("/{}/{}", "tmp", &fname);
    info!("Downloaded to: '{:?}'", &outfile);
    fs::File::create(&outfile).await.unwrap();
    let outfile = PathBuf::from(&outfile);
    let data = response.bytes().await.unwrap();
    let data = data.to_vec();
    fs::write(&outfile, data).await.unwrap();
    let outdir = PathBuf::from("/tmp");
    let mut opt = cli::Opts {
        changesgenerate: false,
        changesauthor: None,
        changesemail: None,
        changesoutfile: None,
        set_version: None,
        set_name: None,
        exclude: None,
        revision: None,
        versionrewriteregex: None,
        versionrewritepattern: None,
        update_crate: vec![],
        no_root_manifest: None,
        respect_lockfile: false,
        custom_root: Some("libflux".to_string()),
        method: Method::Registry,
        src: outfile.to_string_lossy().to_string(),
        compression: Compression::default(),
        tag: Some(random_tag.clone()),
        manifest_path: vec![],
        update: true,
        import_cargo_config: None,
        outdir: outdir.to_path_buf(),
        color: clap::ColorChoice::Auto,
        i_accept_the_risk: vec![],
        vendor_specific_args: None,
    };

    let res = opt.run_vendor();
    assert!(res.is_ok());
    let vendor_tarball = match opt.method {
        Method::Registry => format!("registry-{}.tar.zst", &random_tag),
        Method::Vendor => format!("vendor-{}.tar.zst", &random_tag),
    };

    let vendor_tarball_path = &outdir.join(vendor_tarball);
    assert!(vendor_tarball_path.is_file());

    let raw_outdir = PathBuf::from("/tmp").join(random_tag).join("output");
    let raw_args = RawArgs {
        silent: false,
        target: Some(vendor_tarball_path.to_path_buf()),
        outdir: Some(raw_outdir.clone()),
        subcommands: None,
    };
    raw_opts(raw_args, false)?;
    let vendor_path = raw_outdir.join("libflux").join("vendor");
    let cargo_config_path = raw_outdir
        .join("libflux")
        .join(".cargo")
        .join("config.toml");
    let cargo_lock_path = raw_outdir.join("libflux").join("Cargo.lock");
    let cargo_registry_path = raw_outdir.join(".cargo").join("registry");
    assert!(!vendor_path.is_dir());
    assert!(!cargo_config_path.is_file());
    assert!(cargo_lock_path.is_file());
    assert!(cargo_registry_path.is_dir());
    Ok(())
}

#[test(tokio::test)]
async fn custom_root_test_3() -> io::Result<()> {
    let source = "https://github.com/huggingface/tokenizers/archive/refs/tags/v0.21.0.tar.gz";
    let mut rng = rand::rng();
    let random_tag: u8 = rng.random();
    let random_tag = random_tag.to_string();
    let response = reqwest::get(source).await.unwrap();
    let fname = response
        .url()
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .and_then(|name| if name.is_empty() { None } else { Some(name) })
        .unwrap_or("balls");
    info!("Source file: {}", &fname);
    let outfile = format!("/{}/{}", "tmp", &fname);
    info!("Downloaded to: '{:?}'", &outfile);
    fs::File::create(&outfile).await.unwrap();
    let outfile = PathBuf::from(&outfile);
    let data = response.bytes().await.unwrap();
    let data = data.to_vec();
    fs::write(&outfile, data).await.unwrap();
    let outdir = PathBuf::from("/tmp");
    let manifest_path = vec![
        PathBuf::from("bindings/python/Cargo.toml"),
        PathBuf::from("tokenizers/Cargo.toml"),
    ];

    let mut opt = cli::Opts {
        changesgenerate: false,
        changesauthor: None,
        changesemail: None,
        changesoutfile: None,
        set_version: None,
        set_name: None,
        exclude: None,
        revision: None,
        versionrewriteregex: None,
        versionrewritepattern: None,
        update_crate: vec![],
        no_root_manifest: None,
        respect_lockfile: false,
        custom_root: None,
        method: Method::Registry,
        src: outfile.to_string_lossy().to_string(),
        compression: Compression::default(),
        tag: Some(random_tag.clone()),
        manifest_path,
        update: true,
        import_cargo_config: None,
        outdir: outdir.to_path_buf(),
        color: clap::ColorChoice::Auto,
        i_accept_the_risk: vec![],
        vendor_specific_args: None,
    };

    let res = opt.run_vendor();
    assert!(res.is_ok());
    let vendor_tarball = match opt.method {
        Method::Registry => format!("registry-{}.tar.zst", &random_tag),
        Method::Vendor => format!("vendor-{}.tar.zst", &random_tag),
    };

    let vendor_tarball_path = &outdir.join(vendor_tarball);
    assert!(vendor_tarball_path.is_file());

    let raw_outdir = PathBuf::from("/tmp").join(random_tag).join("output");
    let raw_args = RawArgs {
        silent: false,
        target: Some(vendor_tarball_path.to_path_buf()),
        outdir: Some(raw_outdir.clone()),
        subcommands: None,
    };
    raw_opts(raw_args, false)?;
    let vendor_path = raw_outdir.join("tokenizers").join("vendor");
    let cargo_config_path = raw_outdir
        .join("tokenizers")
        .join(".cargo")
        .join("config.toml");
    let cargo_lock_path = raw_outdir.join("tokenizers").join("Cargo.lock");
    let cargo_registry_path = raw_outdir.join(".cargo").join("registry");
    assert!(!vendor_path.is_dir());
    assert!(!cargo_config_path.is_file());
    assert!(cargo_lock_path.is_file());
    assert!(cargo_registry_path.is_dir());
    Ok(())
}

#[test]
#[ignore]
fn vendor_git_source_of_package_itself_with_vendor_method() -> io::Result<()> {
    let url = "https://github.com/openSUSE-Rust/obs-service-cargo";
    let revision = "v5.1.0";
    let tmp_binding = tempfile::TempDir::new()?;
    let outdir = tmp_binding.path();
    let outdir = outdir.join("obs-service-cargo");
    std::fs::create_dir_all(&outdir)?;
    let specfile_path = std::path::Path::new(MANIFEST_DIR).join("tests/obs-service-cargo.spec");
    std::env::set_current_dir(&outdir)?;
    std::fs::copy(&specfile_path, outdir.join("obs-service-cargo.spec"))?;
    let mut opt = cli::Opts {
        changesgenerate: false,
        changesauthor: None,
        changesemail: None,
        changesoutfile: None,
        set_version: None,
        set_name: None,
        exclude: None,
        revision: Some(revision.to_string()),
        versionrewriteregex: None,
        versionrewritepattern: None,
        update_crate: vec![],
        no_root_manifest: Some(true),
        respect_lockfile: false,
        custom_root: None,
        method: Method::Vendor,
        src: url.to_string(),
        compression: Compression::default(),
        tag: None,
        manifest_path: vec![],
        update: true,
        import_cargo_config: None,
        outdir: outdir.to_path_buf(),
        color: clap::ColorChoice::Auto,
        i_accept_the_risk: vec![],
        vendor_specific_args: None,
    };
    let res = opt.run_vendor();
    assert!(res.is_ok());
    Ok(())
}

#[test]
#[ignore]
fn vendor_git_source_of_package_itself_with_registry_method() -> io::Result<()> {
    let url = "https://github.com/openSUSE-Rust/obs-service-cargo";
    let revision = "v5.1.0";
    let tmp_binding = tempfile::TempDir::new()?;
    let outdir = tmp_binding.path();
    let outdir = outdir.join("obs-service-cargo");
    std::fs::create_dir_all(&outdir)?;
    let specfile_path = std::path::Path::new(MANIFEST_DIR).join("tests/obs-service-cargo.spec");
    std::env::set_current_dir(&outdir)?;
    std::fs::copy(&specfile_path, outdir.join("obs-service-cargo.spec"))?;
    let mut opt = cli::Opts {
        changesgenerate: false,
        changesauthor: None,
        changesemail: None,
        changesoutfile: None,
        set_version: None,
        set_name: None,
        exclude: None,
        revision: Some(revision.to_string()),
        versionrewriteregex: None,
        versionrewritepattern: None,
        update_crate: vec![],
        no_root_manifest: Some(true),
        respect_lockfile: false,
        custom_root: None,
        method: Method::Registry,
        src: url.to_string(),
        compression: Compression::default(),
        tag: None,
        manifest_path: vec![],
        update: true,
        import_cargo_config: None,
        outdir: outdir.to_path_buf(),
        color: clap::ColorChoice::Auto,
        i_accept_the_risk: vec![],
        vendor_specific_args: None,
    };
    let res = opt.run_vendor();
    assert!(res.is_ok());
    Ok(())
}
