use binrw::{BinRead, BinWrite};

#[repr(C)]
#[derive(BinRead, BinWrite)]
#[brw(little)]
pub struct File {
    pub texture_count: u32,
    #[br(count = texture_count)]
    pub textures: Vec<Texture>,
}

#[repr(C)]
#[derive(BinRead, BinWrite)]
#[brw(little, assert(size_0 == size_1), assert(data_count_0 == data_count_1))]
pub struct Texture {
    pub size_0: u32,
    pub size_1: u32,
    pub aspect_ratio: u32,
    pub format: TextureFormat,
    pub unk_0: [u16; 2],
    pub brightness: [u8; 16],
    pub rgb_0: [Rgb; 4],
    pub rgb_1: [Rgb; 4],
    pub unk_1: [u16; 24],
    pub data_count_0: u32,
    pub data_count_1: u32,
    #[br(count = data_count_0)]
    pub data: Vec<u8>,
}

#[repr(u32)]
#[derive(BinRead, BinWrite)]
#[brw(little, repr(u32))]
pub enum TextureFormat {
    RGB8A1 = 1,
    R7G6B5A1 = 11,
    ARGB4 = 12,
}

#[repr(C)]
#[derive(BinRead, BinWrite)]
#[brw(little)]
pub struct Rgb {
    pub r: i16,
    pub g: i16,
    pub b: i16,
}

#[repr(C)]
pub struct TextureProperties {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
}

impl TextureProperties {
    pub fn from_texture(texture: &Texture) -> TextureProperties {
        let size = 1 << (8 - texture.size_1);
        let aspect_ratio = texture.aspect_ratio;
        let stride = match texture.format {
            TextureFormat::R7G6B5A1 | TextureFormat::ARGB4 => 2,
            _ => 1,
        };
        if aspect_ratio > 3 {
            TextureProperties {
                width: size >> (aspect_ratio - 3),
                height: size,
                stride,
            }
        } else {
            TextureProperties {
                width: size,
                height: size >> (3 - aspect_ratio),
                stride,
            }
        }
    }

    fn pixel_count(&self) -> usize {
        (self.width * self.height) as usize
    }

    fn data_length(&self) -> usize {
        (self.width * self.height * self.stride) as usize
    }
}

fn decompress_r7g6b5a1(data: &[u8], properties: &TextureProperties) -> Vec<u8> {
    let mut result = Vec::with_capacity(properties.pixel_count());
    for x in 0..properties.width {
        for y in 0..properties.height {
            let i = 2 * (y + x * properties.height) as usize;
            let p = data[i] as u32 | (data[i + 1] as u32) << 8;
            let r = ((p & 0x7C00) >> 7) as u8;
            let g = ((p & 0x3E0) >> 2) as u8;
            let b = ((p & 0x1F) << 3) as u8;
            let a = match (p & 0x7FFF) == 0 || (p & 0x8000) == 0 {
                true => 0,
                false => 255,
            };
            result.push(r);
            result.push(g);
            result.push(b);
            result.push(a);
        }
    }
    result
}

fn decompress_argb4(data: &[u8], properties: &TextureProperties) -> Vec<u8> {
    let mut result = Vec::with_capacity(properties.pixel_count());
    for x in 0..properties.width {
        for y in 0..properties.height {
            let i = 2 * (y + x * properties.height) as usize;
            let p = data[i] as u32 | (data[i + 1] as u32) << 8;
            let r = ((p & 0xF00) >> 4) as u8;
            let g = (p & 0xF0) as u8;
            let b = ((p & 0xF) << 4) as u8;
            let a = ((p & 0xF000) >> 8) as u8;
            result.push(r);
            result.push(g);
            result.push(b);
            result.push(a);
        }
    }
    result
}

fn decompress_rgb8a1(
    data: &[u8],
    properties: &TextureProperties,
    brightness: &[u8; 16],
    rgb_0: &[Rgb; 4],
    rgb_1: &[Rgb; 4],
) -> Vec<u8> {
    let mut result = Vec::with_capacity(properties.pixel_count());
    for x in 0..properties.width {
        for y in 0..properties.height {
            let i = (y + x * properties.height) as usize;
            let p = data[i];
            // Extract brightness
            let l = brightness[(p >> 4) as usize] as i32;
            // Extract rgb_0
            let i0 = ((p >> 2) % 4) as usize;
            let r0 = ((rgb_0[i0].r << 7) >> 7) as i32;
            let g0 = ((rgb_0[i0].g << 7) >> 7) as i32;
            let b0 = ((rgb_0[i0].b << 7) >> 7) as i32;
            // Extract rgb_1
            let i1 = (p % 4) as usize;
            let r1 = ((rgb_1[i1].r << 7) >> 7) as i32;
            let g1 = ((rgb_1[i1].g << 7) >> 7) as i32;
            let b1 = ((rgb_1[i1].b << 7) >> 7) as i32;
            // Clamp values
            let r = (l + r0 + r1).clamp(0, 255) as u8;
            let g = (l + g0 + g1).clamp(0, 255) as u8;
            let b = (l + b0 + b1).clamp(0, 255) as u8;
            let a = match (r | g | b) == 0 {
                true => 0,
                false => 255,
            };
            result.push(r);
            result.push(g);
            result.push(b);
            result.push(a);
        }
    }
    // 2nd and 3rd pixels are always overwritten by the 1st... probable workaround to an encoder bug
    for i in 1..data.len().min(3) {
        let x = i % properties.width as usize;
        let y = i / properties.height as usize;
        let ri = 4 * (x + y * properties.width as usize);
        result[ri] = result[0];
        result[ri + 1] = result[1];
        result[ri + 2] = result[2];
        result[ri + 3] = result[3];
    }
    result
}

pub fn decompress(texture: &Texture) -> anyhow::Result<Vec<u8>, ()> {
    let properties = TextureProperties::from_texture(texture);
    let expected_data_length = properties.data_length();
    if texture.data.len() != expected_data_length {
        return Err(()); // TODO: Return a reasonable error here
    }
    match texture.format {
        TextureFormat::R7G6B5A1 => Ok(decompress_r7g6b5a1(&texture.data, &properties)),
        TextureFormat::ARGB4 => Ok(decompress_argb4(&texture.data, &properties)),
        TextureFormat::RGB8A1 => Ok(decompress_rgb8a1(
            &texture.data,
            &properties,
            &texture.brightness,
            &texture.rgb_0,
            &texture.rgb_1
        )),
    }
}
