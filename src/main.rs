use std::path::Path;

use anyhow::bail;
use cb_processor::{types::Season, validate_and_print};
use clap::{App, Arg};
use std::fs::File;
use std::str::FromStr;

fn main() -> Result<(), anyhow::Error> {
    let matches = App::new("cb_processor")
        .version("0.0.1")
        .arg(
            Arg::with_name("patch")
            .long("patch")
            .takes_value(false)
            .requires_all(&["hash", "output"])
        )
        .arg(
            Arg::with_name("prime")
            .long("prime")
        )
        .arg(
            Arg::with_name("hash")
            .long("hash")
            .short("h")
            .takes_value(true)
        )
        .arg(
            Arg::with_name("validate")
            .long("validate")
            .takes_value(false)
            .requires_all(&["input", "data-dir", "output"])
            .help("Validates the JSON schema and prints out a short summary of all known recordings and tracks")
        )
        .arg(
            Arg::with_name("convert")
            .conflicts_with("validate")
            .long("convert")
            .takes_value(false)
            .requires_all(&["input", "data-dir", "output"])
            .help("Converts flacs to ogg, if necessary")
        )
        .arg(
            Arg::with_name("input")
            .short("i")
            .long("input")
            .takes_value(true)
            .help("Path to season.json")
        )
        .arg(
            Arg::with_name("data-dir")
                .short("d")
                .long("data")
                .takes_value(true)
                .help("Path to data directory")
                .long_help("Path to data directory\n\nThis is the directory containing the files references in the recordings json file")
        )
        .arg(
            Arg::with_name("metadata")
                .short("m")
                .long("metadata")
                .takes_value(true)
                .help("Path to metadata file")
        )
        .arg(
            Arg::with_name("output")
                .short("o")
                .long("output")
                .takes_value(true)
        )
        .get_matches();

    if matches.is_present("prime") {
        let root_hash = matches.value_of("hash").expect("Missing --hash argument");
        let root_hash = cid::Cid::from_str(root_hash).unwrap();
        cb_processor::ipfs::prime_public_gateways(&root_hash)?;

        return Ok(());
    }

    if matches.is_present("patch") {
        let root_hash = matches.value_of("hash").expect("Missing --hash argument");
        let root_dir = Path::new(matches.value_of("output").expect("Missing --output argument"));
        let root_hash = cid::Cid::from_str(root_hash).unwrap();
        let new_cid = cb_processor::ipfs::patch_root_object(&root_hash, root_dir)?;

        println!("New root object {}", new_cid);
        let b32 = cid::Cid::new_v1(new_cid.codec(), new_cid.hash().to_owned());
        println!(
            "https://{}.ipfs.dweb.link",
            b32.to_string_of_base(multibase::Base::Base32Lower).unwrap()
        );

        return Ok(());
    }

    let season_json_path = Path::new(matches.value_of("input").expect("Missing --input argument"));

    if matches.is_present("validate") {
        let data_dir_path = Path::new(matches.value_of("data-dir").expect("Missing --data argument"));
        let errors_found = validate_and_print(season_json_path, data_dir_path)?;
        if errors_found > 0 {
            bail!("Found {} errors, review the logs above", errors_found);
        } else {
            println!("\nNo errors found");
            return Ok(());
        }
    }

    if matches.is_present("convert") {
        // convert mode needs access to the latest data, we can't run this from metadata
        let data_dir_path = Path::new(matches.value_of("data-dir").expect("Missing --data argument"));
        let season = Season::load(season_json_path, data_dir_path)?;

        cb_processor::convert_all(&season)?;

        return Ok(());
    }

    let season: Season = if let Some(data_dir_path) = matches.value_of("data-dir") {
        Season::load(season_json_path, Path::new(data_dir_path))?
    } else {
        let md_file = matches.value_of("metadata").expect("Missing --data or --metadata argment");
        let f = File::open(md_file)?;
        serde_json::from_reader(f)?
    };

    // Output dir for html and stuff (should probably the same as the --data dir)
    let output_root = Path::new(matches.value_of("output").expect("Missing --output argument"));

    cb_processor::write_season_index(&season, output_root)?;

    cb_processor::write_all_recording_index(&season, output_root)?;

    // write out metadata file
    if let Some(f) = matches.value_of("metadata").and_then(|s| File::create(s).ok()) {
        serde_json::to_writer(f, &season)?;
    }

    Ok(())
}
