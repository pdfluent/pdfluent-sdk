// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! All documents originate here; no input PDFs are read by the generator.
use crate::Result;
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};

pub struct Fixture {
    pub name: &'static str,
    pub features: &'static str,
    pub bytes: Vec<u8>,
    pub pages: usize,
}
struct Builder {
    doc: Document,
    page: ObjectId,
    resources: Dictionary,
}
fn array(values: &[f32]) -> Object {
    Object::Array(values.iter().map(|v| Object::Real(*v)).collect())
}
fn hex(bytes: &[u8]) -> Object {
    Object::String(bytes.to_vec(), lopdf::StringFormat::Hexadecimal)
}
impl Builder {
    fn new(content: impl AsRef<[u8]>) -> Self {
        let mut doc = Document::with_version("1.7");
        // Allocation order and dictionaries are stable. No metadata clock is used.
        let catalog = doc.new_object_id();
        let pages = doc.new_object_id();
        let font = doc
            .add_object(dictionary! {"Type"=>"Font", "Subtype"=>"Type1", "BaseFont"=>"Helvetica"});
        let stream = doc.add_object(Stream::new(dictionary! {}, content.as_ref().to_vec()));
        let page=doc.add_object(dictionary! {"Type"=>"Page", "Parent"=>pages, "MediaBox"=>array(&[0.,0.,240.,180.]), "Contents"=>stream});
        doc.objects.insert(
            pages,
            dictionary! {"Type"=>"Pages", "Kids"=>vec![Object::Reference(page)], "Count"=>1}.into(),
        );
        doc.objects.insert(
            catalog,
            dictionary! {"Type"=>"Catalog", "Pages"=>pages}.into(),
        );
        doc.trailer.set("Root", catalog);
        doc.trailer.set(
            "ID",
            vec![hex(b"visual-suite-001"), hex(b"visual-suite-001")],
        );
        Self {
            doc,
            page,
            resources: dictionary! {"Font"=>dictionary!{"F"=>font}},
        }
    }
    fn resource(&mut self, category: &str, name: &str, object: impl Into<Object>) {
        if !self.resources.has(category.as_bytes()) {
            self.resources.set(category, Dictionary::new());
        }
        self.resources
            .get_mut(category.as_bytes())
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set(name, object);
    }
    fn stream(&mut self, dict: Dictionary, bytes: impl AsRef<[u8]>) -> ObjectId {
        self.doc
            .add_object(Stream::new(dict, bytes.as_ref().to_vec()))
    }
    fn finish(mut self, name: &'static str, features: &'static str) -> Result<Fixture> {
        self.doc
            .get_object_mut(self.page)?
            .as_dict_mut()?
            .set("Resources", self.resources);
        let pages = self.doc.get_pages().len();
        let mut bytes = Vec::new();
        self.doc.save_to(&mut bytes)?;
        Ok(Fixture {
            name,
            features,
            bytes,
            pages,
        })
    }
}
fn simple(name: &'static str, features: &'static str, content: &str) -> Result<Fixture> {
    Builder::new(content).finish(name, features)
}
fn font_fixture(ttf: bool) -> Result<Fixture> {
    let mut b = Builder::new(if ttf {
        "0.1 0.2 0.8 rg BT /F 42 Tf 20 95 Td (ABC CBA) Tj ET"
    } else {
        "0.7 0.1 0.2 rg BT /F 20 Tf 15 100 Td (Latin CFF abc 123) Tj ET"
    });
    let data = if ttf {
        crate::fonts::truetype()
    } else {
        pdf_standard_fonts::StandardFont::Helvetica.data().to_vec()
    };
    let file = b.stream(
        if ttf {
            dictionary! {"Length1"=>data.len() as i64}
        } else {
            dictionary! {"Subtype"=>"Type1C"}
        },
        data,
    );
    let mut desc = dictionary! {"Type"=>"FontDescriptor", "FontName"=>if ttf {"VRPolygon"} else {"Helvetica"}, "Flags"=>32, "FontBBox"=>array(&[0.,-200.,1000.,900.]), "ItalicAngle"=>0, "Ascent"=>800, "Descent"=>-200, "CapHeight"=>700, "StemV"=>80};
    desc.set(if ttf { "FontFile2" } else { "FontFile3" }, file);
    let descriptor = b.doc.add_object(desc);
    let (first, last, width) = if ttf { (65, 67, 650) } else { (32, 126, 600) };
    let font=b.doc.add_object(dictionary!{"Type"=>"Font", "Subtype"=>if ttf {"TrueType"} else {"Type1"}, "BaseFont"=>if ttf {"VRPolygon"} else {"Helvetica"}, "Encoding"=>"WinAnsiEncoding", "FirstChar"=>first, "LastChar"=>last, "Widths"=>vec![Object::Integer(width);(last-first+1) as usize], "FontDescriptor"=>descriptor});
    b.resource("Font", "F", font);
    b.finish(
        if ttf { "text-truetype" } else { "text-cff" },
        if ttf {
            "Embedded original TrueType A/B/C polygon outlines"
        } else {
            "Embedded CFF Latin text; workspace standard-font program"
        },
    )
}
fn shading(kind: i64) -> Result<Fixture> {
    let mut b = Builder::new("q 15 15 210 150 re W n /S sh Q");
    let function = dictionary! {"FunctionType"=>2,"Domain"=>array(&[0.,1.]),"C0"=>array(&[0.9,0.1,0.2]),"C1"=>array(&[0.1,0.3,0.9]),"N"=>1};
    let coords = if kind == 2 {
        array(&[15., 15., 220., 160.])
    } else {
        array(&[85., 85., 5., 125., 90., 100.])
    };
    let s=b.doc.add_object(dictionary!{"ShadingType"=>kind,"ColorSpace"=>"DeviceRGB","Coords"=>coords,"Function"=>function,"Extend"=>vec![true.into(),true.into()]});
    b.resource("Shading", "S", s);
    b.finish(
        if kind == 2 {
            "shading-axial"
        } else {
            "shading-radial"
        },
        if kind == 2 {
            "Type 2 axial shading"
        } else {
            "Type 3 radial shading"
        },
    )
}
fn image_fixture(kind: &'static str) -> Result<Fixture> {
    let mut b = Builder::new("q 180 0 0 120 30 30 cm /Im Do Q");
    let mut data = Vec::new();
    let mut dict = dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>16,"Height"=>16,"BitsPerComponent"=>8};
    let colorspace = match kind {
        "image-gray" | "image-stencil" => Object::Name(b"DeviceGray".to_vec()),
        "image-cmyk" => "DeviceCMYK".into(),
        "image-indexed" => Object::Array(vec![
            "Indexed".into(),
            "DeviceRGB".into(),
            3.into(),
            hex(&[255, 0, 20, 0, 220, 50, 20, 40, 255, 255, 210, 20]),
        ]),
        "image-icc" => {
            // The conversion crate constructs its own deterministic sRGB profile.
            pdf_manip::pdfa_colorspace::add_srgb_output_intent(&mut b.doc)?;
            let root = b.doc.catalog()?;
            let intent = root.get(b"OutputIntents")?.as_array()?[0].as_reference()?;
            let profile = b
                .doc
                .get_dictionary(intent)?
                .get(b"DestOutputProfile")?
                .clone();
            Object::Array(vec!["ICCBased".into(), profile])
        }
        _ => "DeviceRGB".into(),
    };
    dict.set("ColorSpace", colorspace);
    for y in 0..16u8 {
        for x in 0..16u8 {
            match kind {
                "image-gray" => data.push(x * 16),
                "image-cmyk" => data.extend([x * 16, y * 16, 40, 20]),
                "image-indexed" => data.push((x / 4 + y / 4) % 4),
                _ => data.extend([x * 16, y * 16, 220 - x * 8]),
            }
        }
    }
    if kind == "image-smask" {
        let mask: Vec<u8> = (0..256).map(|i| (i % 16 * 17) as u8).collect();
        let id=b.stream(dictionary!{"Type"=>"XObject","Subtype"=>"Image","Width"=>16,"Height"=>16,"BitsPerComponent"=>8,"ColorSpace"=>"DeviceGray"},mask);
        dict.set("SMask", id);
    }
    if kind == "image-stencil" {
        dict.remove(b"ColorSpace");
        dict.set("ImageMask", true);
        dict.set("BitsPerComponent", 1);
        data = (0..32)
            .map(|i| if i % 4 < 2 { 0xaa } else { 0x55 })
            .collect();
    }
    if kind == "image-dct" {
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90).encode(
            &data,
            16,
            16,
            image::ExtendedColorType::Rgb8,
        )?;
        data = jpeg;
        dict.set("Filter", "DCTDecode");
    }
    let mut stream = Stream::new(dict, data);
    if kind != "image-dct" {
        stream.compress()?;
    }
    let id = b.doc.add_object(stream);
    b.resource("XObject", "Im", id);
    b.finish(
        kind,
        match kind {
            "image-rgb" => "DeviceRGB Flate image",
            "image-gray" => "DeviceGray Flate image",
            "image-cmyk" => "DeviceCMYK Flate image",
            "image-indexed" => "Indexed palette Flate image",
            "image-icc" => "ICCBased sRGB image",
            "image-dct" => "Rust-generated DCT JPEG image",
            "image-smask" => "Image with grayscale SMask",
            "image-stencil" => "One-bit stencil image mask",
            _ => unreachable!(),
        },
    )
}
const COLOR_SHAPES: &str =
    "0.9 0.1 0.2 rg 20 25 100 100 re f 0.1 0.3 0.9 rg 85 60 m 215 45 l 180 155 l h f";

pub fn generate() -> Result<Vec<Fixture>> {
    let mut out = vec![font_fixture(true)?, font_fixture(false)?];
    let mut b = Builder::new("");
    let mut content = String::new();
    for (i, font) in pdf_standard_fonts::StandardFont::ALL.iter().enumerate() {
        let id = b.doc.add_object(
            dictionary! {"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>font.postscript_name()},
        );
        b.resource("Font", &format!("F{i}"), id);
        content += &format!(
            "BT /F{i} 9 Tf 10 {} Td (Latin abc 123 XYZ) Tj ET\n",
            170 - i * 12
        );
    }
    let id = b.stream(dictionary! {}, content);
    b.doc
        .get_object_mut(b.page)?
        .as_dict_mut()?
        .set("Contents", id);
    out.push(b.finish(
        "text-standard14",
        "All 14 standard fonts using Type1 font dictionaries",
    )?);
    let mut b = Builder::new("0.1 0.6 0.3 rg BT /T 45 Tf 25 80 Td (AAA) Tj ET");
    let glyph = b.stream(
        dictionary! {},
        "600 0 0 0 600 700 d1 0 0 m 300 700 l 600 0 l h f",
    );
    let font=b.doc.add_object(dictionary!{"Type"=>"Font","Subtype"=>"Type3","FontBBox"=>array(&[0.,0.,600.,700.]),"FontMatrix"=>array(&[0.001,0.,0.,0.001,0.,0.]),"CharProcs"=>dictionary!{"A"=>glyph},"Encoding"=>dictionary!{"Type"=>"Encoding","Differences"=>vec![65.into(),"A".into()]},"FirstChar"=>65,"LastChar"=>65,"Widths"=>vec![600.into()],"Resources"=>Dictionary::new()});
    b.resource("Font", "T", font);
    out.push(b.finish("text-type3", "Original Type3 glyph stream")?);
    out.push(simple("text-render-modes","Text render modes 0–7: fill, stroke, invisible, and clipping", &(0..8).map(|mode|format!("q 0.8 0.1 0.2 rg 0.1 0.2 0.8 RG 0.7 w BT /F 19 Tf {mode} Tr 10 {} Td (Mode {mode}) Tj ET 0.1 0.6 0.4 rg 15 {} 210 9 re f Q\n",160-mode*21,159-mode*21)).collect::<String>())?);
    out.push(simple("text-spacing","Character/word spacing, horizontal scaling, text rise, TJ adjustments","BT /F 20 Tf 12 135 Td 2 Tc 8 Tw (A B C) Tj 0 -40 Td 60 Tz (Scaled words) Tj 0 -40 Td 120 Tz 4 Ts [(ABC) 180 (DEF) -120 (G)] TJ ET")?);
    out.push(simple("vector-fills","Nonzero versus even-odd fill and cubic curves","0.8 0.15 0.1 rg 15 20 90 130 re 35 45 50 80 re f 0.1 0.3 0.8 rg 130 20 90 130 re 150 45 50 80 re f* 1 0.7 0 rg 20 90 m 60 180 180 0 220 90 c 130 80 l h f")?);
    out.push(simple(
        "vector-joins",
        "Miter, round and bevel line joins",
        &(0..3)
            .map(|j| {
                format!(
                    "{j} j 12 w 0.1 0.2 0.8 RG 25 {} m 90 {} l 135 {} l S\n",
                    25 + j * 55,
                    55 + j * 55,
                    25 + j * 55
                )
            })
            .collect::<String>(),
    )?);
    out.push(simple(
        "vector-caps",
        "Butt, round and projecting square line caps",
        &(0..3)
            .map(|j| {
                format!(
                    "{j} J 15 w 0.8 0.1 0.2 RG 40 {} m 200 {} l S\n",
                    30 + j * 55,
                    45 + j * 55
                )
            })
            .collect::<String>(),
    )?);
    out.push(simple("vector-dashes","Dash arrays, nonzero phases and curved dashed strokes","0.1 0.2 0.8 RG 5 w [12 6] 0 d 15 30 m 220 45 l S [3 5 12 5] 7 d 15 85 m 220 85 l S [8 4] 3 d 20 120 m 60 190 170 70 220 150 c S")?);
    out.push(simple("clip-nested","Nested q/Q clips with nonzero and even-odd rules","q 15 15 210 150 re W n 0.8 0.2 0.1 rg 0 0 240 180 re f q 30 30 180 120 re 65 55 100 70 re W* n 0.1 0.3 0.9 rg 0 0 240 180 re f q 0 0 m 240 180 l 0 180 l h W n 0.2 0.8 0.3 rg 0 0 240 180 re f Q Q Q")?);
    out.push(simple("vector-dash-caps", "Dashed strokes with round and square caps and short gaps", "0.15 0.3 0.8 RG 8 w [2 12] 4 d 1 J 20 45 m 220 70 l S 2 J [10 16] 0 d 20 120 m 220 145 l S")?);
    out.push(simple("vector-dash-transform", "Nonuniformly transformed dashed cubic path and dash phase", "q 1.4 0.15 0.2 0.7 10 10 cm 0.8 0.2 0.1 RG 4 w [10 5 2 5] 6 d 10 20 m 40 190 90 0 130 170 c S Q")?);
    out.push(shading(2)?);
    out.push(shading(3)?);
    let mut b = Builder::new("/S sh");
    let data = vec![
        0, 20, 20, 255, 0, 0, 0, 230, 30, 0, 255, 0, 0, 120, 230, 0, 0, 255,
    ];
    let id=b.stream(dictionary!{"ShadingType"=>4,"ColorSpace"=>"DeviceRGB","BitsPerCoordinate"=>8,"BitsPerComponent"=>8,"BitsPerFlag"=>8,"Decode"=>array(&[0.,240.,0.,180.,0.,1.,0.,1.,0.,1.])},data);
    b.resource("Shading", "S", id);
    out.push(b.finish("shading-mesh", "Type 4 free-form Gouraud triangle mesh")?);
    let mut b = Builder::new("/Pattern cs /P scn 10 10 220 160 re f");
    let id=b.stream(dictionary!{"Type"=>"Pattern","PatternType"=>1,"PaintType"=>1,"TilingType"=>1,"BBox"=>array(&[0.,0.,16.,16.]),"XStep"=>16,"YStep"=>16,"Resources"=>Dictionary::new()},"0.1 0.3 0.8 rg 0 0 8 8 re f 0.9 0.2 0.1 rg 8 8 8 8 re f");
    b.resource("Pattern", "P", id);
    out.push(b.finish("pattern-tiling", "Colored tiling pattern")?);
    for name in [
        "image-rgb",
        "image-gray",
        "image-cmyk",
        "image-indexed",
        "image-icc",
        "image-dct",
        "image-smask",
        "image-stencil",
    ] {
        out.push(image_fixture(name)?);
    }
    for (name, mode) in [
        ("blend-multiply", "Multiply"),
        ("blend-screen", "Screen"),
        ("blend-overlay", "Overlay"),
        ("blend-difference", "Difference"),
    ] {
        let mut b = Builder::new(
            "0.8 0.2 0.1 rg 15 15 150 130 re f /G gs 0.1 0.3 0.9 rg 75 50 150 115 re f",
        );
        b.resource(
            "ExtGState",
            "G",
            dictionary! {"Type"=>"ExtGState","BM"=>mode,"ca"=>0.65,"CA"=>0.45},
        );
        out.push(b.finish(name, "Constant fill/stroke alpha and named blend mode")?);
    }
    let mut b = Builder::new("0.3 0.7 0.2 rg 0 0 240 180 re f /Group Do");
    let id=b.stream(dictionary!{"Type"=>"XObject","Subtype"=>"Form","BBox"=>array(&[0.,0.,240.,180.]),"Group"=>dictionary!{"S"=>"Transparency","CS"=>"DeviceRGB","I"=>true,"K"=>true},"Resources"=>dictionary!{"ExtGState"=>dictionary!{"G"=>dictionary!{"ca"=>0.6}}}},format!("/G gs {COLOR_SHAPES}"));
    b.resource("XObject", "Group", id);
    out.push(b.finish(
        "transparency-group",
        "Isolated and knockout transparency group",
    )?);
    for (name, rotate) in [
        ("page-rotate90", 90),
        ("page-rotate180", 180),
        ("page-rotate270", 270),
    ] {
        let mut b = Builder::new(COLOR_SHAPES);
        b.doc
            .get_object_mut(b.page)?
            .as_dict_mut()?
            .set("Rotate", rotate);
        out.push(b.finish(name, "Asymmetric colored paths and page rotation")?);
    }
    let mut b = Builder::new(COLOR_SHAPES);
    b.doc
        .get_object_mut(b.page)?
        .as_dict_mut()?
        .set("CropBox", array(&[30., 20., 210., 150.]));
    out.push(b.finish("page-crop", "Offset CropBox smaller than MediaBox")?);
    let mut b = Builder::new("/Outer Do");
    let mut child=b.stream(dictionary!{"Type"=>"XObject","Subtype"=>"Form","BBox"=>array(&[0.,0.,240.,180.]),"Resources"=>Dictionary::new()},COLOR_SHAPES);
    for _ in 0..2 {
        child=b.stream(dictionary!{"Type"=>"XObject","Subtype"=>"Form","BBox"=>array(&[0.,0.,240.,180.]),"Matrix"=>array(&[0.85,0.,0.,0.85,10.,10.]),"Resources"=>dictionary!{"XObject"=>dictionary!{"Child"=>child}}},"/Child Do");
    }
    b.resource("XObject", "Outer", child);
    out.push(b.finish(
        "forms-nested",
        "Three levels of form XObjects with transforms",
    )?);
    let mut b = Builder::new("0.9 g 0 0 240 180 re f");
    let ap=b.stream(dictionary!{"Type"=>"XObject","Subtype"=>"Form","BBox"=>array(&[0.,0.,180.,40.]),"Resources"=>b.resources.clone()},"1 1 1 rg 0 0 180 40 re f 0.1 0.2 0.8 RG 2 w 1 1 178 38 re S 0 g BT /F 18 Tf 10 12 Td (Widget value) Tj ET");
    let widget=b.doc.add_object(dictionary!{"Type"=>"Annot","Subtype"=>"Widget","FT"=>"Tx","T"=>Object::string_literal("example"),"V"=>Object::string_literal("Widget value"),"Rect"=>array(&[30.,70.,210.,110.]),"F"=>4,"P"=>b.page,"AP"=>dictionary!{"N"=>ap}});
    b.doc
        .get_object_mut(b.page)?
        .as_dict_mut()?
        .set("Annots", vec![widget.into()]);
    b.doc.catalog_mut()?.set("AcroForm",dictionary!{"Fields"=>vec![widget.into()],"NeedAppearances"=>false,"DR"=>b.resources.clone()});
    out.push(b.finish(
        "acroform-widget",
        "AcroForm field and widget with explicit normal appearance",
    )?);
    let mut b = Builder::new("0.95 g 0 0 240 180 re f");
    let mut annots = Vec::new();
    for (i, kind) in ["Stamp", "Square", "FreeText"].iter().enumerate() {
        let ap=b.stream(dictionary!{"Type"=>"XObject","Subtype"=>"Form","BBox"=>array(&[0.,0.,200.,40.]),"Resources"=>b.resources.clone()},format!("0.8 0.1 0.2 RG 2 w 1 1 198 38 re S 0.1 0.2 0.8 rg BT /F 18 Tf 8 12 Td ({kind}) Tj ET"));
        let y = 15. + i as f32 * 55.;
        let id=b.doc.add_object(dictionary!{"Type"=>"Annot","Subtype"=>*kind,"Rect"=>array(&[20.,y,220.,y+40.]),"Contents"=>Object::string_literal(*kind),"F"=>4,"AP"=>dictionary!{"N"=>ap}});
        annots.push(id.into());
    }
    b.doc
        .get_object_mut(b.page)?
        .as_dict_mut()?
        .set("Annots", Object::Array(annots));
    out.push(b.finish(
        "annotations",
        "Stamp, square and free-text annotations with appearance streams",
    )?);
    let source = simple(
        "color-shapes",
        "Opaque asymmetric colored paths",
        COLOR_SHAPES,
    )?;
    let mut doc = Document::load_mem(&source.bytes)?;
    let mut bytes = Vec::new();
    pdf_manip::encrypt::encrypt_and_save(
        &mut doc,
        &pdf_manip::encrypt::EncryptConfig {
            user_password: vec![],
            owner_password: b"owner".to_vec(),
            algorithm: pdf_manip::encrypt::EncryptionAlgorithm::Rc4_40,
            permissions: Default::default(),
        },
        &mut bytes,
    )?;
    out.push(Fixture {
        name: "encrypted-rc4",
        features: "RC4-40 revision 2, empty user password",
        bytes,
        pages: 1,
    });
    let mut doc = Document::load_mem(&source.bytes)?;
    crate::encryption::aes256(&mut doc);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes)?;
    out.push(Fixture {
        name: "encrypted-aes256",
        features: "AES-256 revision 5, fixed test salts/IV, empty user password",
        bytes,
        pages: 1,
    });
    let mut doc = Document::load_mem(&source.bytes)?;
    pdf_manip::watermark::apply_text_watermark(
        &mut doc,
        &pdf_manip::watermark::TextWatermark {
            text: "DRAFT".into(),
            font_size: 32.,
            ..Default::default()
        },
        &pdf_manip::watermark::PageSelection::All,
    )?;
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes)?;
    out.push(Fixture {
        name: "watermark",
        features: "pdf-manip text watermark applied to color-shapes",
        bytes,
        pages: 1,
    });
    let bytes = pdf_manip::pdfa::convert_bytes(&source.bytes, &Default::default())?;
    // Conversion resides in pdf-manip; compliance identifies the converted output.
    let parsed = pdf_syntax::Pdf::new(bytes.clone()).map_err(|_| "converted PDF does not parse")?;
    assert!(pdf_compliance::detect_pdfa_level(&parsed).is_some());
    out.push(Fixture {
        name: "pdfa-converted",
        features: "pdf-manip PDF/A conversion; pdf-compliance identification",
        bytes,
        pages: 1,
    });
    out.push(source);
    Ok(out)
}
