// SPDX-License-Identifier: MPL-2.0

// Copyright (C) 2024 To all Contributors of this project listed in CONTRIBUTORS.md

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use clap::Parser;
use obs_service_cargo::cli;

use std::io;
use std::io::IsTerminal;

use terminfo::{Database, capability as cap};
#[allow(unused_imports)]
use tracing::{Level, debug, error, info, warn};
use tracing_subscriber::EnvFilter;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = cli::Opts::parse();

    let terminfodb = Database::from_env().map_err(|e| {
        error!(err = ?e, "Unable to access terminfo db. This is a bug!");
        io::Error::other(
            "Unable to access terminfo db. This is a bug! Setting color option to false!",
        )
    });

    let is_termcolorsupported = match terminfodb {
        Ok(hasterminfodb) => hasterminfodb.get::<cap::MaxColors>().is_some(),
        Err(_) => false,
    };

    let to_color = std::io::stdout().is_terminal()
        && match &args.color {
            clap::ColorChoice::Auto => is_termcolorsupported,
            clap::ColorChoice::Always => true,
            clap::ColorChoice::Never => false,
        };

    let filter_layer = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let builder = tracing_subscriber::fmt()
        .with_level(true)
        .with_ansi(to_color)
        .with_env_filter(filter_layer)
        .with_level(true);

    let builder = if cfg!(debug_assertions) {
        builder.with_file(true).with_line_number(true)
    } else {
        builder
    };

    builder.init();

    info!("🎢 Starting OBS Service Cargo Vendor.");
    debug!(?args);

    Ok(args.run_vendor().map_err(|err| {
        error!("{}", err);
        err
    })?)
}
