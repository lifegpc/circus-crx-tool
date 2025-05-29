pub mod advdata;
pub mod args;
pub mod crx;
pub mod ext;
pub mod pck;
pub mod utils;

pub fn auto(input: &str) -> anyhow::Result<()> {
    let pb = std::path::PathBuf::from(input);
    let ext = pb
        .extension()
        .unwrap_or(std::ffi::OsStr::new(""))
        .to_ascii_lowercase();
    if pb.is_dir() {
        if ext == "pck" {
            let pck_name = pb.file_name().ok_or(anyhow::anyhow!(
                "Failed to get file name from path: {}",
                pb.display()
            ))?;
            let ori_pck_file_loc = advdata::ADV_DATA_MAP
                .get(pck_name.to_string_lossy().as_ref())
                .ok_or(anyhow::anyhow!(
                    "No advdata found for file: {}",
                    pck_name.to_string_lossy()
                ))?;
            let output_path = advdata::BASE_PATH.join("patched").join(
                ori_pck_file_loc
                    .to_string_lossy()
                    .strip_prefix(&advdata::BASE_PATH.to_string_lossy().into_owned())
                    .map(|s| s.trim_start_matches("/").trim_start_matches("\\"))
                    .ok_or(anyhow::anyhow!(
                        "Failed to strip base path from filename: {}",
                        ori_pck_file_loc.display()
                    ))?,
            );
            utils::make_sure_dir_exists(&output_path)?;
            let mut reader = pck::PckReader::new_from_file(&ori_pck_file_loc)?;
            let mut writer = pck::PckWriter::new_from_file(
                &output_path,
                pck::PckWriter::calculate_header_size(reader.len() as u32),
            )?;
            for mut i in reader.iter_mut() {
                let op = pb.join(&i.header.name).with_extension("png");
                let mut f = writer.add_file(&i.header.name)?;
                if op.exists() {
                    let size = i.header.size as u64;
                    let mut crx = crx::Crx::read_from(&mut i, || Ok(size))?;
                    crx.import_png(&op)?;
                    crx.write_to(&mut f)?;
                } else {
                    eprintln!("File {} does not exist, skipping import.", op.display());
                    std::io::copy(&mut i, &mut f)?;
                }
            }
            writer.write_header()?;
            eprintln!("Exported PCK to: {}", output_path.display());
            return Ok(());
        }
        for entry in std::fs::read_dir(pb)? {
            let entry = entry?;
            auto(&entry.path().to_string_lossy())?;
        }
        return Ok(());
    }
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
        if let Some(parent) = pb.parent() {
            if parent
                .file_name()
                .is_some_and(|f| advdata::ADV_DATA_MAP.contains_key(f.to_string_lossy().as_ref()))
            {
                return auto(parent.to_string_lossy().as_ref());
            }
        }
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
    } else if ext == "pck" {
        let mut pck = pck::PckReader::new_from_file(&pb)?;
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
            pb2
        } else {
            let mut p = pb2.join("extracted");
            loop {
                match removed.pop() {
                    Some(name) => p.push(name),
                    None => break,
                }
            }
            p
        };
        std::fs::create_dir_all(&output_path)?;
        for mut i in pck.iter_mut() {
            let len = i.header.size as u64;
            let crx = crx::Crx::read_from(&mut i, || Ok(len))?;
            let op = output_path.join(&i.header.name).with_extension("png");
            crx.export_png(&op)?;
        }
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

pub fn unpack(input: &str, output: &str) -> anyhow::Result<()> {
    let mut pck = pck::PckReader::new_from_file(input)?;
    std::fs::create_dir_all(output)?;
    for mut i in pck.iter_mut() {
        let op = std::path::PathBuf::from(output).join(&i.header.name);
        let f = std::fs::File::create(&op)?;
        let mut writer = std::io::BufWriter::new(f);
        std::io::copy(&mut i, &mut writer)?;
    }
    Ok(())
}

pub fn pack(input: &str, output: &str) -> anyhow::Result<()> {
    let input_path = std::path::PathBuf::from(input);
    if input_path.is_dir() {
        let mut paths = Vec::new();
        for entry in std::fs::read_dir(input_path)? {
            let entry = entry?;
            if entry.path().is_file() {
                paths.push((entry.path(), entry.file_name()));
            }
        }
        let mut pck = pck::PckWriter::new_from_file(
            output,
            pck::PckWriter::calculate_header_size(paths.len() as u32),
        )?;
        for entry in paths {
            let file_name = entry.1;
            let mut writer = pck.add_file(&file_name.to_string_lossy())?;
            let mut f = std::fs::File::open(entry.0)?;
            std::io::copy(&mut f, &mut writer)?;
        }
        pck.write_header()?;
    } else if input_path.is_file() {
        let mut pck = pck::PckWriter::new_from_file(output, 0x800)?;
        let file_name = input_path
            .file_name()
            .ok_or(anyhow::anyhow!("No filename"))?;
        let mut writer = pck.add_file(&file_name.to_string_lossy())?;
        let mut f = std::fs::File::open(input_path)?;
        std::io::copy(&mut f, &mut writer)?;
        pck.write_header()?;
    }
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
            args::Command::Unpack { input, output } => unpack(input, output).unwrap(),
            args::Command::Pack { input, output } => pack(input, output).unwrap(),
        }
    }
}
