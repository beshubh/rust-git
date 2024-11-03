use anyhow;
use clap::{Parser, Subcommand};
use flate2::write::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use hex;
use sha1::{Digest, Sha1};
#[allow(unused_imports)]
use std::env;
#[allow(unused_imports)]
use std::fs;
use std::io;
use std::io::prelude::*;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum GitError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("fatal: invalid object type: {0}")]
    OType(String),

    #[error("UTF-8 conversion error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),

    #[error("Object parsing error: {0}")]
    Parse(String),

    #[error("Invalid object hash: {0}")]
    InvalidHash(String),
}

type Result<T> = anyhow::Result<T, GitError>;

#[derive(Debug)]
struct GitObject {
    type_: String,
    content: String,
    size: usize,
}

fn read_tree_object(data: &Vec<u8>) -> Result<String> {
    // NOTE: we don't completely implement's git's ls-tree we are just showing the name
    // but with our implementaion it's not that hard to implement full ls-tree
    let mut entries = Vec::new();
    let mut position = 0;
    while position < data.len() {
        let null_pos = data[position..]
            .iter()
            .position(|&b| b == 0)
            .ok_or(GitError::Parse("Invalid object format".into()))?;
        let mode_and_name = String::from_utf8(data[position..position + null_pos].to_vec())?;
        let (_mode, name) = mode_and_name
            .split_once(' ')
            .ok_or(GitError::Parse("Invalid ojbectf format".into()))?;
        position += null_pos + 1;

        if position + 20 > data.len() {
            return Err(GitError::Parse("Incomplete SHA".into()));
        }
        let _sha = hex::encode(&data[position..position + 20]);
        position += 20;
        entries.push(name.to_string());
    }
    Ok(entries.join("\n"))
}

impl GitObject {
    fn from_raw(raw_bytes: Vec<u8>) -> Result<Self> {
        let null_pos = raw_bytes
            .iter()
            .position(|&b| b == 0)
            .ok_or(GitError::Parse("Invalid object format".into()))?;
        let header = String::from_utf8(raw_bytes[..null_pos].to_vec())?;
        let parts: Vec<&str> = header.split_whitespace().collect();
        if parts.len() != 2 {
            return Err(GitError::Parse("Invalid header format".into()));
        }
        let content = raw_bytes[null_pos + 1..].to_vec();
        let type_ = parts[0].to_string();
        let size = parts[1]
            .parse()
            .map_err(|_| GitError::Parse("Invalid Size".into()))?;

        match type_.as_str() {
            "blob" => Ok(GitObject {
                type_,
                size,
                content: String::from_utf8(content.into())?,
            }),
            "tree" => Ok(GitObject {
                type_,
                size,
                content: read_tree_object(&content.into())?,
            }),
            default => Err(GitError::OType(default.into())),
        }
    }
}

struct GitRepo {
    path: PathBuf,
}

impl GitRepo {
    fn new() -> Self {
        GitRepo {
            path: PathBuf::from(".git"),
        }
    }

    fn init(&self) -> Result<()> {
        fs::create_dir(".git").unwrap();
        fs::create_dir(".git/objects").unwrap();
        fs::create_dir(".git/refs").unwrap();
        fs::write(".git/HEAD", "ref: refs/heads/main\n").unwrap();
        println!("Initialized git directory");
        Ok(())
    }

    fn read_object(&self, hash: &str) -> Result<GitObject> {
        if hash.len() < 2 {
            return Err(GitError::InvalidHash("Hash too short".into()));
        }
        let (dir, file) = hash.split_at(2);
        let object_path = self.path.join("objects").join(dir).join(file);
        let compressed_data = fs::read(&object_path)?;
        let decompressed = self.decompress_object(&compressed_data)?;
        Ok(GitObject::from_raw(decompressed)?)
    }

    fn decompress_object(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut writer = Vec::new();
        let mut decoder = ZlibDecoder::new(writer);
        decoder.write_all(data)?;
        writer = decoder.finish()?;
        Ok(writer)
    }

    fn compress_content(&self, byte_sequence: &Vec<u8>) -> Result<Vec<u8>> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(byte_sequence)?;
        let comprssed_bytes = encoder.finish()?;
        Ok(comprssed_bytes)
    }

    fn hash_object(&self, object_byte_sequence: &Vec<u8>, object_type: String) -> Result<String> {
        let mut hasher = Sha1::new();
        hasher.update(object_type.as_bytes());
        hasher.update(b" ");
        hasher.update(object_byte_sequence.len().to_string().as_bytes());
        hasher.update(b"\0");
        hasher.update(object_byte_sequence);
        let result = hasher.finalize();
        Ok(format!("{:x}", result))
    }

    fn write_object(&self, hash: &str, byte_sequence: &Vec<u8>) -> Result<()> {
        let compressed = self.compress_content(byte_sequence)?;
        let (dir, file) = hash.split_at(2);
        fs::create_dir(self.path.join("objects").join(dir))?;
        fs::write(self.path.join("objects").join(dir).join(file), compressed)?;
        Ok(())
    }

    fn ls_tree(&self, tree_ish: &str) -> Result<String> {
        let git_object = self.read_object(tree_ish)?;
        if git_object.type_ != String::from("tree") {
            return Err(GitError::OType(git_object.type_));
        }
        let output = git_object.content;
        Ok(output)
    }

    fn write_tree(&self, dir_path: Option<&str>) -> Result<String> {
        let mut p = ".";
        if dir_path.is_some() {
            p = dir_path.unwrap();
        }

        let mut tree_entries = Vec::new();

        let dir_entries = fs::read_dir(p)?;
        for dir_entry in dir_entries {
            let dir_entry = dir_entry?;
            let path = dir_entry.path();
            let filename = path
                .file_name()
                .ok_or_else(|| GitError::Parse("invalid filename".into()))?
                .to_string_lossy()
                .into_owned();
            if filename == String::from(".git") {
                continue;
            }
            if path.is_dir() {
                let hash = self.write_tree(Option::Some(path.to_str().unwrap()))?;
                tree_entries.push(["40000".to_string(), filename, "\0".to_string(), hash]);
            } else if path.is_file() {
                let object_byte_sequence = fs::read(path)?;
                let hash = self.hash_object(&object_byte_sequence, "blob".into())?;
                let byte_sequence_with_header = [
                    b"blob ",
                    object_byte_sequence.len().to_string().as_bytes(),
                    b"\0",
                    &object_byte_sequence,
                ]
                .concat();
                self.write_object(&hash, &byte_sequence_with_header)?;
                tree_entries.push(["100644".to_string(), filename, "\0".to_string(), hash]);
            }
        }
        tree_entries.sort_by(|a, b| a[1].cmp(&b[1]));
        let tree_entries_bytes = tree_entries
            .iter()
            .map(|entry| {
                [
                    entry[0].as_bytes(),
                    b" ",
                    entry[1].as_bytes(),
                    entry[2].as_bytes(),
                    &hex::decode(entry[3].clone()).unwrap(),
                ]
                .concat()
            })
            .collect::<Vec<_>>()
            .concat();

        let hash = self.hash_object(&tree_entries_bytes, "tree".into())?;
        let tree_byte_sequence = [
            b"tree ",
            tree_entries_bytes.len().to_string().as_bytes(),
            b"\0",
            &tree_entries_bytes,
        ]
        .concat();
        self.write_object(&hash, &tree_byte_sequence)?;
        Ok(hash)
    }
}

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
    #[command(name = "hash-object")]
    HashObject {
        #[arg(short = 'w', group = "mode")]
        write: bool,

        filename: String,
    },
    #[command(name = "ls-tree")]
    LsTree {
        #[arg(long)]
        name_only: bool,
        tree_ish: String,
    },
    #[command(name = "write-tree")]
    WriteTree,
}

fn run_command(command: &Commands) -> Result<()> {
    let repo = GitRepo::new();
    match command {
        Commands::Init => repo.init(),
        Commands::CatFile {
            pretty,
            type_,
            size,
            exists,
            object_hash,
        } => {
            let object = repo.read_object(&object_hash)?;
            match (pretty, type_, size, exists) {
                (true, _, _, _) => {
                    print!("{}", object.content);
                    Ok(())
                }
                (_, true, _, _) => {
                    println!("{}", object.type_);
                    Ok(())
                }
                (_, _, true, _) => {
                    println!("{}", object.size);
                    Ok(())
                }
                (_, _, _, true) => Ok(()),
                _ => Err(GitError::Parse("No mode specified".into())),
            }
        }
        Commands::HashObject { write, filename } => {
            if *write {
                let hash = repo.hash_object(&fs::read(filename)?, "blob".into())?;
                let file_content = fs::read(filename)?;
                let byte_sequence: Vec<u8> = [
                    b"blob ",
                    file_content.len().to_string().as_bytes(),
                    b"\0",
                    &file_content,
                ]
                .concat();
                repo.write_object(&hash, &byte_sequence)?;
                print!("{}", hash);
                Ok(())
            } else {
                let hash = repo.hash_object(&fs::read(filename)?, "blob".into())?;
                print!("{}", hash);
                Ok(())
            }
        }
        Commands::LsTree { tree_ish, .. } => {
            let output = repo.ls_tree(&tree_ish)?;
            println!("{}", output);
            Ok(())
        }
        Commands::WriteTree => {
            let output = repo.write_tree(None)?;
            print!("{}", output);
            Ok(())
        }
    }
}

fn main() {
    let cli = Cli::parse();
    // if let Err(err) = run_command(&cli.command) {
    //     eprintln!("{}", err);
    // }
    run_command(&cli.command).unwrap();
}
