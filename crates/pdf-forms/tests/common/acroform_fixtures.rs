//! Shared synthetic AcroForm fixtures for the corpus gate, the file emitter
//! (`examples/gen_acroform_corpus.rs`) and the binding tests.
//!
//! Each fixture is a minimal-but-valid single-page PDF exercising one
//! AcroForm category from the support contract. Built in-memory with lopdf so
//! the set is reproducible and self-contained (no binary blobs in git).
//!
//! Lives under `tests/common/` so cargo does NOT compile it as its own test
//! binary; consumers pull it in with `#[path = "common/acroform_fixtures.rs"]`.

#![allow(dead_code)] // each consumer uses a different subset

use lopdf::{dictionary, Document, Object, ObjectId, Stream, StringFormat};

/// Builder for a one-page PDF whose page annotations are the supplied fields.
pub struct FormPdf {
    doc: Document,
    pages_id: ObjectId,
    page_id: ObjectId,
    field_ids: Vec<ObjectId>,
    need_appearances: bool,
    xfa: bool,
}

impl FormPdf {
    pub fn new() -> Self {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let content_id = doc.add_object(Stream::new(dictionary! {}, Vec::new()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! {},
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        Self {
            doc,
            pages_id,
            page_id,
            field_ids: Vec::new(),
            need_appearances: false,
            xfa: false,
        }
    }

    pub fn need_appearances(mut self) -> Self {
        self.need_appearances = true;
        self
    }

    pub fn with_xfa(mut self) -> Self {
        self.xfa = true;
        self
    }

    pub fn add(&mut self, dict: lopdf::Dictionary) -> ObjectId {
        let id = self.doc.add_object(Object::Dictionary(dict));
        self.field_ids.push(id);
        id
    }

    /// Add a field given an already-allocated object id (for hierarchical
    /// parents whose kids reference the parent).
    pub fn add_ref(&mut self, id: ObjectId) {
        self.field_ids.push(id);
    }

    pub fn doc_mut(&mut self) -> &mut Document {
        &mut self.doc
    }

    pub fn new_id(&mut self) -> ObjectId {
        self.doc.new_object_id()
    }

    /// A checkbox/radio `/AP /N` substate dictionary mapping `on_state` and
    /// `Off` to tiny appearance streams.
    pub fn button_ap(&mut self, on_state: &[u8]) -> Object {
        let on = self
            .doc
            .add_object(Stream::new(dictionary! {}, b"q Q".to_vec()));
        let off = self
            .doc
            .add_object(Stream::new(dictionary! {}, b"q Q".to_vec()));
        let mut n = lopdf::Dictionary::new();
        n.set(on_state.to_vec(), Object::Reference(on));
        n.set(b"Off".to_vec(), Object::Reference(off));
        Object::Dictionary(dictionary! { "N" => Object::Dictionary(n) })
    }

    pub fn finish(mut self) -> Vec<u8> {
        let refs: Vec<Object> = self
            .field_ids
            .iter()
            .map(|&id| Object::Reference(id))
            .collect();
        if let Ok(Object::Dictionary(page)) = self.doc.get_object_mut(self.page_id) {
            page.set("Annots", Object::Array(refs.clone()));
        }
        let mut acro = dictionary! {
            "Fields" => Object::Array(refs),
            "DA" => Object::String(b"/Helv 0 Tf 0 g".to_vec(), StringFormat::Literal),
        };
        if self.need_appearances {
            acro.set("NeedAppearances", Object::Boolean(true));
        }
        if self.xfa {
            // A static-XFA shell: an inert template packet alongside a real
            // AcroForm. parse_acroform must still read the AcroForm side.
            let xdp = b"<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\"><template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\"><subform name=\"form1\"/></template></xdp:xdp>".to_vec();
            let xfa_stream = self.doc.add_object(Stream::new(dictionary! {}, xdp));
            acro.set("XFA", Object::Reference(xfa_stream));
        }
        let acroform_id = self.doc.add_object(Object::Dictionary(acro));
        let catalog_id = self.doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(self.pages_id),
            "AcroForm" => Object::Reference(acroform_id),
        });
        self.doc.trailer.set("Root", Object::Reference(catalog_id));
        let mut buf = Vec::new();
        self.doc.save_to(&mut buf).expect("serialise fixture");
        buf
    }
}

impl Default for FormPdf {
    fn default() -> Self {
        Self::new()
    }
}

fn text_field(name: &str, flags: i64, max_len: Option<i64>) -> lopdf::Dictionary {
    let mut d = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Tx",
        "T" => Object::String(name.as_bytes().to_vec(), StringFormat::Literal),
        "Rect" => vec![100.into(), 700.into(), 320.into(), 720.into()],
    };
    if flags != 0 {
        d.set("Ff", Object::Integer(flags));
    }
    if let Some(ml) = max_len {
        d.set("MaxLen", Object::Integer(ml));
    }
    d
}

fn choice_field(name: &str, flags: i64, opts: &[&str]) -> lopdf::Dictionary {
    let opt_arr: Vec<Object> = opts
        .iter()
        .map(|o| Object::String(o.as_bytes().to_vec(), StringFormat::Literal))
        .collect();
    dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Ch",
        "T" => Object::String(name.as_bytes().to_vec(), StringFormat::Literal),
        "Ff" => Object::Integer(flags),
        "Rect" => vec![100.into(), 400.into(), 320.into(), 520.into()],
        "Opt" => Object::Array(opt_arr),
    }
}

/// One labelled fixture: (category id, the field name to fill, the value kind,
/// the PDF bytes).
pub struct Fixture {
    pub category: &'static str,
    pub bytes: Vec<u8>,
}

/// Build the full representative corpus. Categories map 1:1 to the support
/// contract's field types and edge cases.
pub fn all_fixtures() -> Vec<Fixture> {
    vec![
        Fixture {
            category: "pure_text",
            bytes: pure_text(),
        },
        Fixture {
            category: "multiline_comb",
            bytes: multiline_comb(),
        },
        Fixture {
            category: "checkbox",
            bytes: checkbox(),
        },
        Fixture {
            category: "radio",
            bytes: radio(),
        },
        Fixture {
            category: "combo",
            bytes: combo(),
        },
        Fixture {
            category: "listbox_single",
            bytes: listbox_single(),
        },
        Fixture {
            category: "multiselect",
            bytes: multiselect(),
        },
        Fixture {
            category: "nonascii",
            bytes: nonascii(),
        },
        Fixture {
            category: "needappearances_missing_ap",
            bytes: needappearances_missing_ap(),
        },
        Fixture {
            category: "signature",
            bytes: signature(),
        },
        Fixture {
            category: "xfa_shell",
            bytes: xfa_shell(),
        },
        Fixture {
            category: "hierarchical",
            bytes: hierarchical(),
        },
        Fixture {
            category: "readonly",
            bytes: readonly(),
        },
    ]
}

pub fn pure_text() -> Vec<u8> {
    let mut b = FormPdf::new();
    b.add(text_field("full_name", 0, None));
    b.finish()
}

pub fn multiline_comb() -> Vec<u8> {
    let mut b = FormPdf::new();
    b.add(text_field("notes", 0x1000, None)); // Multiline
    b.add(text_field("postcode", 0x1000000, Some(6))); // Comb + MaxLen 6
    b.finish()
}

pub fn checkbox() -> Vec<u8> {
    let mut b = FormPdf::new();
    let ap = b.button_ap(b"Yes");
    let mut f = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Btn",
        "T" => Object::String(b"agree".to_vec(), StringFormat::Literal),
        "V" => Object::Name(b"Off".to_vec()),
        "AS" => Object::Name(b"Off".to_vec()),
        "Rect" => vec![100.into(), 600.into(), 112.into(), 612.into()],
    };
    f.set("AP", ap);
    b.add(f);
    b.finish()
}

pub fn radio() -> Vec<u8> {
    // A radio group with two kid widgets (on-states "M" and "F").
    let mut b = FormPdf::new();
    let parent_id = b.new_id();
    let ap_m = b.button_ap(b"M");
    let ap_f = b.button_ap(b"F");
    let kid_m = b.doc_mut().add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "Parent" => parent_id,
        "AS" => Object::Name(b"Off".to_vec()),
        "Rect" => vec![100.into(), 560.into(), 112.into(), 572.into()],
    });
    if let Ok(Object::Dictionary(d)) = b.doc_mut().get_object_mut(kid_m) {
        d.set("AP", ap_m);
    }
    let kid_f = b.doc_mut().add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "Parent" => parent_id,
        "AS" => Object::Name(b"Off".to_vec()),
        "Rect" => vec![130.into(), 560.into(), 142.into(), 572.into()],
    });
    if let Ok(Object::Dictionary(d)) = b.doc_mut().get_object_mut(kid_f) {
        d.set("AP", ap_f);
    }
    b.doc_mut().objects.insert(
        parent_id,
        Object::Dictionary(dictionary! {
            "FT" => "Btn",
            "Ff" => Object::Integer(0x8000), // Radio
            "T" => Object::String(b"gender".to_vec(), StringFormat::Literal),
            "V" => Object::Name(b"Off".to_vec()),
            "Kids" => vec![kid_m.into(), kid_f.into()],
        }),
    );
    // Top-level field = the parent; page widgets = the two kids.
    b.finish_keep_annots(vec![parent_id.into()], vec![kid_m.into(), kid_f.into()])
}

pub fn combo() -> Vec<u8> {
    let mut b = FormPdf::new();
    // Combo (bit 18) non-editable.
    b.add(choice_field("country", 0x20000, &["US", "NL", "DE"]));
    b.finish()
}

pub fn listbox_single() -> Vec<u8> {
    let mut b = FormPdf::new();
    // Plain list box (no Combo, no MultiSelect).
    b.add(choice_field("city", 0, &["Amsterdam", "Berlin", "Paris"]));
    b.finish()
}

pub fn multiselect() -> Vec<u8> {
    let mut b = FormPdf::new();
    // MultiSelect (bit 22 = 0x200000).
    b.add(choice_field(
        "languages",
        0x200000,
        &["EN", "NL", "DE", "FR"],
    ));
    b.finish()
}

pub fn nonascii() -> Vec<u8> {
    let mut b = FormPdf::new();
    b.add(text_field("latin1_name", 0, None)); // for "Café Zürich" (WinAnsi)
    b.add(text_field("cjk_name", 0, None)); // for "日本語" (UTF-16BE + NeedAppearances)
    b.finish()
}

pub fn needappearances_missing_ap() -> Vec<u8> {
    let mut b = FormPdf::new().need_appearances();
    // Text field already carrying a /V but no /AP — the "filled by a tool that
    // only wrote /V" pattern that regenerate_appearances must repair.
    let mut f = text_field("prefilled", 0, None);
    f.set(
        "V",
        Object::String(b"existing value".to_vec(), StringFormat::Literal),
    );
    b.add(f);
    b.finish()
}

pub fn signature() -> Vec<u8> {
    let mut b = FormPdf::new();
    b.add(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Sig",
        "T" => Object::String(b"signature1".to_vec(), StringFormat::Literal),
        "Rect" => vec![100.into(), 300.into(), 300.into(), 360.into()],
    });
    b.finish()
}

pub fn xfa_shell() -> Vec<u8> {
    let mut b = FormPdf::new().with_xfa();
    // A real AcroForm text field living alongside a static-XFA template shell.
    b.add(text_field("acro_name", 0, None));
    b.finish()
}

pub fn hierarchical() -> Vec<u8> {
    // Parent "address" with named kids "street" and "city" → FQNs
    // "address.street", "address.city".
    let mut b = FormPdf::new();
    let parent_id = b.new_id();
    let street = b.doc_mut().add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Tx",
        "Parent" => parent_id,
        "T" => Object::String(b"street".to_vec(), StringFormat::Literal),
        "Rect" => vec![100.into(), 700.into(), 320.into(), 720.into()],
    });
    let city = b.doc_mut().add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Tx",
        "Parent" => parent_id,
        "T" => Object::String(b"city".to_vec(), StringFormat::Literal),
        "Rect" => vec![100.into(), 670.into(), 320.into(), 690.into()],
    });
    b.doc_mut().objects.insert(
        parent_id,
        Object::Dictionary(dictionary! {
            "T" => Object::String(b"address".to_vec(), StringFormat::Literal),
            "Kids" => vec![street.into(), city.into()],
        }),
    );
    b.finish_keep_annots(vec![parent_id.into()], vec![street.into(), city.into()])
}

pub fn readonly() -> Vec<u8> {
    let mut b = FormPdf::new();
    b.add(text_field("locked", 0x1, None)); // ReadOnly
    b.finish()
}

impl FormPdf {
    /// Finish with explicit /AcroForm /Fields (top-level) and page /Annots
    /// (widgets), for hierarchical/radio fixtures where the field tree and the
    /// annotation list differ.
    fn finish_keep_annots(mut self, fields: Vec<Object>, widget_annots: Vec<Object>) -> Vec<u8> {
        if let Ok(Object::Dictionary(page)) = self.doc.get_object_mut(self.page_id) {
            page.set("Annots", Object::Array(widget_annots));
        }
        let mut acro = dictionary! {
            "Fields" => Object::Array(fields),
            "DA" => Object::String(b"/Helv 0 Tf 0 g".to_vec(), StringFormat::Literal),
        };
        if self.need_appearances {
            acro.set("NeedAppearances", Object::Boolean(true));
        }
        let acroform_id = self.doc.add_object(Object::Dictionary(acro));
        let catalog_id = self.doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(self.pages_id),
            "AcroForm" => Object::Reference(acroform_id),
        });
        self.doc.trailer.set("Root", Object::Reference(catalog_id));
        let mut buf = Vec::new();
        self.doc.save_to(&mut buf).expect("serialise fixture");
        buf
    }
}
