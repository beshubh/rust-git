use clap::{Parser, Subcommand};
use flate2::bufread::GzDecoder;
use flate2::write::ZlibDecoder;
#[allow(unused_imports)]
use std::env;
#[allow(unused_imports)]
use std::fs;
use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::path::PathBuf;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init,
    #[command(name = "cat-file")]
    CatFile {
        #[arg(short = 'p', group = "mode")]
        pretty: bool,
        /// Show type
        #[arg(short = 't', group = "mode")]
        type_: bool,
        /// Show size
        #[arg(short = 's', group = "mode")]
        size: bool,
        /// Check if exists
        #[arg(short = 'e', group = "mode")]
        exists: bool,
        /// The object hash
        object_hash: String,
    },
}

fn parse_blob(content: String) -> Option<(String, String)> {
    let null_pos = content.find('\0')?;
    let (header, content) = content.split_at(null_pos);
    let content = &content[1..];
    Some((header.to_string(), content.to_string()))
}
fn read_compressed_file(path: &str) -> Result<String, Box<dyn std::error::Error>> {
    // Open the file
    let file = fs::read(path).unwrap();
    let mut writer = Vec::new();

    let mut decoder = ZlibDecoder::new(writer);
    decoder.write_all(&file).unwrap();
    writer = decoder.finish().unwrap();
    let content = String::from_utf8(writer).expect("Error parsing to string, line 57");
    Ok(content)
}

fn read_file(object_hash: String) -> Result<String, Box<dyn std::error::Error>> {
    let (dir, hash) = object_hash.split_at(2);
    let mut path = PathBuf::new();
    path.push("./.git/objects/");
    path.push(dir);
    path.push(hash);
    let res = read_compressed_file(path.to_str().unwrap());
    if let Ok(content) = res {
        Ok(content)
    } else {
        Err(res.unwrap_err())
    }
}

fn pretty_print(object_hash: String) -> Result<String, Box<dyn std::error::Error>> {
    let content = read_file(object_hash)?;
    let parse_res = parse_blob(content);
    if let Some((_header, content)) = parse_res {
        Ok(content)
    } else {
        Ok(String::from("something"))
    }
}

fn object_type(_object_hash: String) -> io::Result<()> {
    unimplemented!()
}

fn object_exists(_object_hash: String) -> io::Result<()> {
    unimplemented!()
}

fn object_size(_object_hash: String) -> io::Result<()> {
    unimplemented!();
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Init => {
            fs::create_dir(".git").unwrap();
            fs::create_dir(".git/objects").unwrap();
            fs::create_dir(".git/refs").unwrap();
            fs::write(".git/HEAD", "ref: refs/heads/main\n").unwrap();
            println!("Initialized git directory")
        }
        Commands::CatFile {
            pretty,
            type_,
            size,
            exists,
            object_hash,
        } => {
            if *pretty {
                let content = pretty_print(object_hash.to_string()).unwrap();
                print!("{}", content);
            } else if *type_ {
                let _ = object_type(object_hash.to_string());
            } else if *size {
                let _ = object_size(object_hash.to_string());
            } else if *exists {
                let _ = object_exists(object_hash.to_string());
            }
        }
    }
}
