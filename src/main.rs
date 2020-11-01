use std::path::Path;

use anyhow::bail;
use cb_processor::{types::{Season}, validate_and_print};
use clap::{App, Arg};


fn main() -> Result<(), anyhow::Error> {
    let matches = App::new("cb_processor")
        .version("0.0.1")
        .arg(
            Arg::with_name("validate")
            .long("validate")
            .takes_value(false)
            .help("Validates the JSON schema and prints out a short summary of all known recordings and tracks")
        )
        .arg(
            Arg::with_name("input")
            .short("i")
            .long("input")
            .takes_value(true)
            .required(true)
            .help("Path to season.json")
        )
        .arg(
            Arg::with_name("data-dir")
                .short("d")
                .long("data")
                .takes_value(true)
                .required(true)
                .help("Path to data directory")
                .long_help("Path to data directory\n\nThis is the directory containing the files references in the recordings json file")
        )
        .arg(
            Arg::with_name("output")
                .short("o")
                .long("output")
                .takes_value(true)
        )
        .get_matches();

    let season_json_path = Path::new(matches.value_of("input").expect("Missing --input argument"));
    let data_dir_path = Path::new(matches.value_of("data-dir").expect("Missing --data argument"));

    if matches.is_present("validate") {
        let errors_found = validate_and_print(season_json_path, data_dir_path)?;
        if errors_found > 0 {
            bail!("Found {} errors, review the logs above", errors_found);
        } else {
            println!("\nNo errors found");
            return Ok(());
        }
    }

    let output_root = Path::new(matches.value_of("output").expect("Missing --output argument"));

    let season = Season::load(season_json_path, data_dir_path)?;
    
    cb_processor::write_season_index(&season, output_root, data_dir_path)?;

    cb_processor::write_all_recording_index(&season, output_root, data_dir_path)?;
    


    Ok(())
}
