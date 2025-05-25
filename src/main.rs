pub mod advdata;
pub mod args;
pub mod crx;
pub mod ext;
pub mod utils;

pub fn auto(input: &str) -> anyhow::Result<()> {
    let pb = std::path::PathBuf::from(input);
    let ext = pb
        .extension()
        .unwrap_or(std::ffi::OsStr::new(""))
        .to_ascii_lowercase();
    if ext == "crx" {
        let crx = crx::Crx::read_from_file(&pb)?;
        let mut pb2 = pb.clone();
        let mut failed = false;
        let mut removed = Vec::new();
        while pb2
            .file_name()
            .is_some_and(|f| f.to_ascii_lowercase() != "advdata")
        {
            pb2.file_name().map(|s| removed.push(s.to_owned()));
            if !pb2.pop() {
                failed = true;
                eprintln!(
                    "Failed to find 'advdata' directory in path: {}",
                    pb.display()
                );
                break;
            }
        }
        if !failed {
            pb2.file_name().map(|s| removed.push(s.to_owned()));
            pb2.pop();
        }
        let output_path = if failed {
            pb2.with_extension("png")
        } else {
            let mut p = pb2.join("extracted");
            loop {
                match removed.pop() {
                    Some(name) => p.push(name),
                    None => break,
                }
            }
            p.with_extension("png")
        };
        utils::make_sure_dir_exists(&output_path)?;
        crx.export_png(&output_path)?;
    } else if ext == "png" {
        let filename = pb.file_name().ok_or(anyhow::anyhow!(
            "Failed to get file name from path: {}",
            pb.display()
        ))?;
        let mut crx_filename = std::path::PathBuf::from(filename);
        crx_filename.set_extension("crx");
        let crx_filename = crx_filename
            .file_name()
            .ok_or(anyhow::anyhow!("No filename"))?
            .to_string_lossy()
            .to_string();
        println!("{}", crx_filename);
        let data = advdata::ADV_DATA_MAP
            .get(crx_filename.as_str())
            .ok_or(anyhow::anyhow!(
                "No advdata found for file: {}",
                filename.display()
            ))?;
        let mut crx = crx::Crx::read_from_file(data)?;
        crx.import_png(&pb)?;
        let output_path = advdata::BASE_PATH.join("patched").join(
            data.to_string_lossy()
                .strip_prefix(&advdata::BASE_PATH.to_string_lossy().into_owned())
                .map(|s| s.trim_start_matches("/").trim_start_matches("\\"))
                .ok_or(anyhow::anyhow!(
                    "Failed to strip base path from filename: {}",
                    data.display()
                ))?,
        );
        println!("{}", output_path.display());
        utils::make_sure_dir_exists(&output_path)?;
        crx.write_to_file(&output_path)?;
    }
    Ok(())
}

pub fn export_crx(input: &str, output: &str) -> anyhow::Result<()> {
    let crx = crx::Crx::read_from_file(input)?;
    utils::make_sure_dir_exists(&output)?;
    crx.export_png(&output)?;
    Ok(())
}

pub fn import_crx(origin: &str, input: &str, output: &str) -> anyhow::Result<()> {
    let mut crx = crx::Crx::read_from_file(origin)?;
    crx.import_png(input)?;
    utils::make_sure_dir_exists(&output)?;
    crx.write_to_file(output)?;
    Ok(())
}

fn main() {
    let args = args::Arg::parse();
    unsafe { std::env::set_var("RUST_LIB_BACKTRACE", "1") };
    if let Some(arg) = args.auto.as_ref() {
        let e = match auto(&arg.input) {
            Ok(_) => {
                eprintln!("Auto operation completed successfully.");
                false
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                eprintln!("Backtrace: {}", e.backtrace());
                true
            }
        };
        if e {
            eprintln!("Press Enter to exit program.");
            let mut s = String::new();
            let _ = std::io::stdin().read_line(&mut s);
        }
    }
    if let Some(command) = args.command.as_ref() {
        match command {
            args::Command::Export { input, output } => export_crx(input, output).unwrap(),
            args::Command::Import {
                origin,
                input,
                output,
            } => {
                import_crx(origin, input, output).unwrap();
            }
        }
    }
}
