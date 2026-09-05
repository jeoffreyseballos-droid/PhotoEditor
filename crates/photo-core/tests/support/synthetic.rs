use std::{fs, path::Path};
pub fn synthetic_dng(path: &Path) {
    use tiff::{
        encoder::{colortype::Gray16, TiffEncoder},
        tags::Tag,
    };
    let mut file = fs::File::create(path).unwrap();
    let mut encoder = TiffEncoder::new(&mut file).unwrap();
    let mut image = encoder.new_image::<Gray16>(128, 96).unwrap();
    let tags = image.encoder();
    tags.write_tag(Tag::PhotometricInterpretation, 32803u16)
        .unwrap();
    tags.write_tag(Tag::Make, "PhotoEditor Test Camera")
        .unwrap();
    tags.write_tag(Tag::Model, "Synthetic Bayer").unwrap();
    tags.write_tag(
        Tag::ImageDescription,
        "C:/private/source/path - do not copy",
    )
    .unwrap();
    tags.write_tag(Tag::Unknown(50706), [1u8, 4, 0, 0].as_slice())
        .unwrap();
    tags.write_tag(Tag::Unknown(50707), [1u8, 1, 0, 0].as_slice())
        .unwrap();
    tags.write_tag(Tag::Unknown(50708), "PhotoEditor synthetic Bayer fixture")
        .unwrap();
    tags.write_tag(Tag::Unknown(33421), [2u16, 2].as_slice())
        .unwrap();
    tags.write_tag(Tag::Unknown(33422), [0u8, 1, 1, 2].as_slice())
        .unwrap();
    tags.write_tag(Tag::Unknown(50717), 4095u32).unwrap();
    tags.write_tag(Tag::Unknown(50714), 64u16).unwrap();
    tags.write_tag(
        Tag::Unknown(50721),
        [1f64, 0., 0., 0., 1., 0., 0., 0., 1.].as_slice(),
    )
    .unwrap();
    tags.write_tag(Tag::Unknown(50728), [1f64, 1., 1.].as_slice())
        .unwrap();
    tags.write_tag(Tag::Unknown(50778), 21u16).unwrap();
    let pixels: Vec<u16> = (0..128 * 96)
        .map(|i| 512 + ((i % 128) * 12) as u16)
        .collect();
    image.write_data(&pixels).unwrap();
}
