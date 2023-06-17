pub mod gex;

use std::{env, fs::{File, create_dir_all}, io::BufReader, path::PathBuf};

use binrw::BinReaderExt;
use gex::vfx;

fn extract_vfx(path: &PathBuf) {
    let file = File::open(path).unwrap();
    let vfx: vfx::File = BufReader::new(file).read_le().unwrap();
    let level_name = path.file_stem().unwrap();
    let output_path = path.parent().unwrap().to_path_buf().join(level_name);
    create_dir_all(&output_path).unwrap();
    for (index, texture) in vfx.textures.iter().enumerate() {
        let properties = vfx::TextureProperties::from_texture(texture);
        let buffer = vfx::decompress(texture).unwrap();
        let format = match texture.format {
            vfx::TextureFormat::RGB8A1 => "rgb8a1",
            vfx::TextureFormat::R7G6B5A1 => "r7g6b5a1",
            vfx::TextureFormat::ARGB4 => "argb4",
        };
        let output_path = output_path.join(format!("{index}_{format}.png"));
        println!("{output_path:?}");
        image::save_buffer(
            &output_path,
            &buffer,
            properties.width,
            properties.height,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}

fn main() {
    let paths: Vec<PathBuf> = env::args()
        .map(PathBuf::from)
        .filter(|x| x.exists() && x.is_file())
        .collect();

    for filepath in &paths {
        if let Some(os_str) = filepath.extension() {
            if let Some("vfx") = os_str.to_str() {
                extract_vfx(filepath);
            }
        }
    }
}
