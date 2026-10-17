//! Preserve source geometry when embedding substitute fonts.
//!
//! PDF/A requires the PDF width and embedded glyph width to agree. Retaining an
//! incorrect `/Widths` array fails validation. An unembedded substitute is
//! fitted to the declared widths by viewers; embedding it removes that fit.
//! Explicit horizontal scaling preserves it without manufacturing word gaps.
//! Already embedded programs with contradictory widths keep the existing repair
//! route: their painted outline, declared advance and extracted word boundaries
//! can disagree, so width deltas alone do not prove a faithful rewrite.
// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use crate::content_editor::ContentEditor;
use lopdf::{content::Operation, Dictionary, Document, Object, ObjectId};
use std::collections::{BTreeMap, BTreeSet};

type Fonts = BTreeMap<Vec<u8>, ObjectId>;
type Widths = BTreeMap<ObjectId, BTreeMap<u8, f32>>;

pub(crate) struct Snapshot {
    widths: Widths,
    substitute_fonts: BTreeSet<ObjectId>,
    page_streams: BTreeSet<ObjectId>,
    streams: BTreeMap<ObjectId, (Fonts, Vec<Operation>)>,
}

fn dict<'a>(doc: &'a Document, obj: &'a Object) -> Option<&'a Dictionary> {
    doc.dereference(obj).ok()?.1.as_dict().ok()
}

fn widths(doc: &Document) -> Widths {
    doc.objects
        .iter()
        .filter_map(|(&id, obj)| {
            let font = obj.as_dict().ok()?;
            if !matches!(
                font.get(b"Subtype").ok()?.as_name().ok()?,
                b"TrueType" | b"Type1" | b"MMType1"
            ) {
                return None;
            }
            let first = font.get(b"FirstChar").ok()?.as_i64().ok()?;
            let array = doc
                .dereference(font.get(b"Widths").ok()?)
                .ok()?
                .1
                .as_array()
                .ok()?;
            let values = array
                .iter()
                .enumerate()
                .filter_map(|(i, value)| {
                    let code = u8::try_from(first.checked_add(i64::try_from(i).ok()?)?).ok()?;
                    let width = value.as_float().ok()?;
                    (width.is_finite() && width >= 0.0).then_some((code, width))
                })
                .collect();
            Some((id, values))
        })
        .collect()
}

fn resources(doc: &Document, mut page: ObjectId) -> Option<&Dictionary> {
    let mut seen = BTreeSet::new();
    while seen.insert(page) {
        let d = doc.get_dictionary(page).ok()?;
        if let Ok(r) = d.get(b"Resources") {
            return dict(doc, r);
        }
        page = d.get(b"Parent").ok()?.as_reference().ok()?;
    }
    None
}

fn font_map(doc: &Document, resources: &Dictionary) -> Option<Fonts> {
    // A gs operator may set a font independently of Tf. Until that case has
    // its own state resolver, retain the original content in these contexts.
    if let Some(states) = resources.get(b"ExtGState").ok().and_then(|o| dict(doc, o)) {
        if states
            .iter()
            .any(|(_, o)| dict(doc, o).is_none_or(|d| d.has(b"Font")))
        {
            return None;
        }
    }
    let fonts = dict(doc, resources.get(b"Font").ok()?)?;
    fonts
        .iter()
        .map(|(name, obj)| Some((name.clone(), obj.as_reference().ok()?)))
        .collect()
}

fn text_state(op: &Operation) -> bool {
    matches!(
        op.operator.as_str(),
        "q" | "Q"
            | "gs"
            | "BT"
            | "ET"
            | "Tf"
            | "Tj"
            | "TJ"
            | "'"
            | "\""
            | "Tc"
            | "Tw"
            | "Tz"
            | "TL"
            | "Tr"
            | "Ts"
            | "Tm"
            | "Td"
            | "TD"
            | "T*"
    )
}

impl Snapshot {
    pub(crate) fn capture(doc: &Document) -> Self {
        let mut page_streams = BTreeSet::new();
        let mut contexts: BTreeMap<ObjectId, Option<Fonts>> = BTreeMap::new();
        let mut add = |id, fonts: Option<Fonts>| {
            contexts
                .entry(id)
                .and_modify(|old| {
                    if *old != fonts {
                        *old = None;
                    }
                })
                .or_insert(fonts);
        };
        for page in doc.get_pages().values() {
            let fonts = resources(doc, *page).and_then(|r| font_map(doc, r));
            let ids = crate::content_editor::get_content_stream_ids(doc, *page);
            // The pipeline joins complete page contents before this snapshot.
            // Unjoined fragments can inherit text state: do not guess it.
            for id in &ids {
                if ids.len() == 1 {
                    page_streams.insert(*id);
                }
                add(*id, if ids.len() == 1 { fonts.clone() } else { None });
            }
        }
        for (&id, obj) in &doc.objects {
            if let Object::Stream(stream) = obj {
                if let Ok(r) = stream.dict.get(b"Resources") {
                    page_streams.remove(&id);
                    add(id, dict(doc, r).and_then(|r| font_map(doc, r)));
                }
            }
        }
        let streams = contexts
            .into_iter()
            .filter_map(|(id, fonts)| {
                let fonts = fonts?;
                let stream = doc.get_object(id).ok()?.as_stream().ok()?;
                let bytes = stream.get_plain_content().ok()?;
                let editor = ContentEditor::from_stream_strict(&bytes).ok()?;
                Some((
                    id,
                    (
                        fonts,
                        editor
                            .operations()
                            .iter()
                            .filter(|o| text_state(o))
                            .cloned()
                            .collect(),
                    ),
                ))
            })
            .collect();
        let substitute_fonts = doc
            .objects
            .iter()
            .filter_map(|(&id, obj)| {
                let font = obj.as_dict().ok()?;
                let embedded = font
                    .get(b"FontDescriptor")
                    .ok()
                    .and_then(|o| dict(doc, o))
                    .is_some_and(|d| {
                        [b"FontFile".as_slice(), b"FontFile2", b"FontFile3"]
                            .iter()
                            .any(|k| d.has(k))
                    });
                (!embedded).then_some(id)
            })
            .collect();
        Self {
            widths: widths(doc),
            substitute_fonts,
            page_streams,
            streams,
        }
    }

    pub(crate) fn restore(self, doc: &mut Document) -> usize {
        let current_snapshot = Self::capture(doc);
        let current = &current_snapshot.widths;
        // Viewers fit unembedded substitute glyphs to the declared widths. Once
        // embedded, that implicit fit disappears. Preserve it explicitly on
        // complete page streams, where the initial text state is known.
        let fits: Widths = self
            .widths
            .iter()
            .filter_map(|(id, old)| {
                if !self.substitute_fonts.contains(id) {
                    return None;
                }
                let now = current.get(id)?;
                let ratios = old
                    .iter()
                    .filter_map(|(code, width)| {
                        let new = *now.get(code)?;
                        let ratio = *width / new;
                        (new > 0.0
                            && *width > 0.0
                            && ratio.is_finite()
                            && (new - width).abs() > 0.01)
                            .then_some((*code, ratio))
                    })
                    .collect();
                Some((*id, ratios))
            })
            .collect();
        let deltas: Widths = self
            .widths
            .into_iter()
            .filter_map(|(id, old)| {
                if !self.substitute_fonts.contains(&id) {
                    return None;
                }
                let now = current.get(&id)?;
                let delta: BTreeMap<_, _> = old
                    .into_iter()
                    .filter_map(|(code, width)| {
                        let d = now.get(&code)? - width;
                        (d.abs() > 0.01).then_some((code, d))
                    })
                    .collect();
                (!delta.is_empty()).then_some((id, delta))
            })
            .collect();
        let mut changed = 0;
        let no_fits = BTreeMap::new();
        for (id, (fonts, original)) in self.streams {
            if current_snapshot
                .streams
                .get(&id)
                .is_none_or(|(now, _)| *now != fonts)
            {
                continue;
            }
            if !fonts.values().any(|f| deltas.contains_key(f)) {
                continue;
            }
            let Some(stream) = doc.objects.get(&id).and_then(|o| o.as_stream().ok()) else {
                continue;
            };
            let Ok(bytes) = stream.get_plain_content() else {
                continue;
            };
            let Ok(editor) = ContentEditor::from_stream_strict(&bytes) else {
                continue;
            };
            let state: Vec<_> = editor
                .operations()
                .iter()
                .filter(|o| text_state(o))
                .cloned()
                .collect();
            // If another repair changed codes, ordering or text state, this
            // stream needs a dedicated correspondence proof before adjustment.
            if state.len() != original.len()
                || state
                    .iter()
                    .zip(&original)
                    .any(|(a, b)| a.operator != b.operator || a.operands != b.operands)
            {
                continue;
            }
            let Some(operations) = compensate(
                editor.operations(),
                &fonts,
                &deltas,
                if self.page_streams.contains(&id) {
                    &fits
                } else {
                    &no_fits
                },
            ) else {
                continue;
            };
            let Ok(bytes) = ContentEditor::from_operations(operations).encode() else {
                continue;
            };
            if let Some(Object::Stream(stream)) = doc.objects.get_mut(&id) {
                stream.set_plain_content(bytes);
                stream.dict.remove(b"DecodeParms");
                changed += 1;
            }
        }
        changed
    }
}

#[derive(Clone, Copy)]
struct TextState {
    font: Option<ObjectId>,
    char_space: f32,
    word_space: f32,
    scale: f32,
}

fn compensate(
    ops: &[Operation],
    fonts: &Fonts,
    deltas: &Widths,
    fits: &Widths,
) -> Option<Vec<Operation>> {
    let mut state = TextState {
        font: None,
        char_space: 0.0,
        word_space: 0.0,
        scale: 100.0,
    };
    let mut stack = Vec::new();
    let mut out = Vec::new();
    let mut changed = false;
    for op in ops {
        match op.operator.as_str() {
            "Tf" => {
                state.font = op
                    .operands
                    .first()
                    .and_then(|o| o.as_name().ok())
                    .and_then(|n| fonts.get(n))
                    .copied();
            }
            "Tc" => state.char_space = op.operands.first()?.as_float().ok()?,
            "Tw" => state.word_space = op.operands.first()?.as_float().ok()?,
            "Tz" => state.scale = op.operands.first()?.as_float().ok()?,
            "q" => stack.push(state),
            "Q" => state = stack.pop()?,
            "Tj" | "TJ" | "'" | "\"" => {
                if op.operator == "\"" {
                    state.word_space = op.operands.first()?.as_float().ok()?;
                    state.char_space = op.operands.get(1)?.as_float().ok()?;
                }
                if let Some(delta) = state.font.and_then(|f| deltas.get(&f)) {
                    let fit = state.font.and_then(|f| fits.get(&f));
                    let items = match op.operator.as_str() {
                        "TJ" => op.operands.first()?.as_array().ok()?.clone(),
                        "\"" => vec![op.operands.get(2)?.clone()],
                        _ => vec![op.operands.first()?.clone()],
                    };
                    let mut array = Vec::new();
                    let mut replacement = Vec::new();
                    let mut adjusted = false;
                    for item in items {
                        if let Object::String(bytes, format) = item {
                            let mut run = Vec::new();
                            for c in bytes {
                                if let Some(ratio) = fit.and_then(|f| f.get(&c)) {
                                    // Scaling also scales Tc/Tw. Divide those by
                                    // the same factor so original spacing and
                                    // the next text position remain unchanged.
                                    let scale = state.scale * ratio;
                                    let tc = state.char_space / ratio;
                                    let tw = state.word_space / ratio;
                                    if ![scale, tc, tw].iter().all(|v| v.is_finite()) {
                                        return None;
                                    }
                                    if !run.is_empty() {
                                        array
                                            .push(Object::String(std::mem::take(&mut run), format));
                                    }
                                    if !array.is_empty() {
                                        replacement.push(Operation::new(
                                            "TJ",
                                            vec![Object::Array(std::mem::take(&mut array))],
                                        ));
                                    }
                                    replacement.push(Operation::new("Tz", vec![scale.into()]));
                                    if state.char_space != 0.0 {
                                        replacement.push(Operation::new("Tc", vec![tc.into()]));
                                    }
                                    if state.word_space != 0.0 {
                                        replacement.push(Operation::new("Tw", vec![tw.into()]));
                                    }
                                    replacement.push(Operation::new(
                                        "Tj",
                                        vec![Object::String(vec![c], format)],
                                    ));
                                    replacement
                                        .push(Operation::new("Tz", vec![state.scale.into()]));
                                    if state.char_space != 0.0 {
                                        replacement.push(Operation::new(
                                            "Tc",
                                            vec![state.char_space.into()],
                                        ));
                                    }
                                    if state.word_space != 0.0 {
                                        replacement.push(Operation::new(
                                            "Tw",
                                            vec![state.word_space.into()],
                                        ));
                                    }
                                    adjusted = true;
                                } else {
                                    run.push(c);
                                    if let Some(d) = delta.get(&c) {
                                        array
                                            .push(Object::String(std::mem::take(&mut run), format));
                                        array.push(Object::Real(*d));
                                        adjusted = true;
                                    }
                                }
                            }
                            if !run.is_empty() {
                                array.push(Object::String(run, format));
                            }
                        } else {
                            array.push(item);
                        }
                    }
                    if adjusted {
                        if op.operator == "\"" {
                            out.push(Operation::new("Tw", vec![op.operands.first()?.clone()]));
                            out.push(Operation::new("Tc", vec![op.operands.get(1)?.clone()]));
                        }
                        if matches!(op.operator.as_str(), "'" | "\"") {
                            out.push(Operation::new("T*", vec![]));
                        }
                        out.extend(replacement);
                        if !array.is_empty() {
                            out.push(Operation::new("TJ", vec![Object::Array(array)]));
                        }
                        changed = true;
                        continue;
                    }
                }
            }
            _ => {}
        }
        out.push(op.clone());
    }
    changed.then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    fn owned_document() -> (Document, ObjectId, ObjectId) {
        let mut doc = Document::with_version("1.7");
        let font = doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
            "FirstChar" => 65, "LastChar" => 65, "Widths" => vec![1000.into()],
            "Encoding" => "WinAnsiEncoding",
        });
        let stream = doc.add_object(lopdf::Stream::new(
            dictionary! {},
            b"BT /F 12 Tf 10 10 Td (AA) Tj ET".to_vec(),
        ));
        let pages = doc.new_object_id();
        let page = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages, "Contents" => stream,
            "MediaBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
            "Resources" => dictionary! {"Font" => dictionary! {"F" => font}},
        });
        doc.objects.insert(
            pages,
            dictionary! {"Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1}.into(),
        );
        let root = doc.add_object(dictionary! {"Type" => "Catalog", "Pages" => pages});
        doc.trailer.set("Root", root);
        (doc, font, stream)
    }

    /// The glyph after this one must land where the author put it.
    ///
    /// This asserted the MECHANISM: that the declared width is corrected to the
    /// face's own and an explicit horizontal scale put back what that took
    /// away. There are two ways to keep the geometry and that is one of them.
    /// The other, added for #380, is to leave the declared width alone and
    /// re-metric the substitute program to it -- which reaches text inside Form
    /// XObjects, where a content-stream fit does not, and which is why govdocs
    /// `001_001672` had 67 of its 84 widths moved with no compensation at all.
    ///
    /// So the assertion is now the invariant both serve: whatever the file does
    /// about it, code 65 occupies the 1000 units the document declared. A test
    /// that names one implementation forbids the other one from existing.
    #[test]
    fn conversion_repairs_font_widths_without_moving_the_next_glyph() {
        let (mut doc, font, stream) = owned_document();
        crate::pdfa::convert_document(&mut doc, &Default::default()).unwrap();
        let width = widths(&doc)[&font][&65];
        let bytes = doc
            .get_object(stream)
            .unwrap()
            .as_stream()
            .unwrap()
            .get_plain_content()
            .unwrap();
        let editor = ContentEditor::from_stream_strict(&bytes).unwrap();
        let scales: Vec<_> = editor
            .operations()
            .iter()
            .filter(|o| o.operator == "Tz")
            .map(|o| o.operands[0].as_float().unwrap())
            .collect();
        let drawn = match scales.len() {
            // Re-metricked: the declared width is already the program's, so
            // nothing had to be scaled.
            0 => width,
            // Fitted: two glyphs, each scaled and then set back to 100.
            4 => {
                assert_eq!(
                    scales[1], 100.0,
                    "the scale must be put back after the glyph"
                );
                assert_eq!(scales[2], scales[0], "both glyphs are fitted the same way");
                assert_eq!(scales[3], 100.0);
                width * scales[0] / 100.0
            }
            n => panic!("{n} horizontal scale operators; expected none or one pair per glyph"),
        };
        assert!(
            (drawn - 1000.0).abs() < 0.01,
            "code 65 draws {drawn} units where the document declared 1000, so the glyph \
             after it moved -- which is the whole point of this pass, however it is done"
        );
    }

    #[test]
    fn quote_operators_and_saved_font_state_keep_their_spacing() {
        let editor = ContentEditor::from_stream(
            b"/F 12 Tf (A) ' q /G 10 Tf (A) Tj Q 3 2 (A) \" [(A) 120 (B)] TJ",
        )
        .unwrap();
        let fonts = BTreeMap::from([(b"F".to_vec(), (1, 0)), (b"G".to_vec(), (2, 0))]);
        let deltas = BTreeMap::from([((1, 0), BTreeMap::from([(b'A', 100.0)]))]);
        let result = compensate(editor.operations(), &fonts, &deltas, &BTreeMap::new()).unwrap();
        let names: Vec<_> = result.iter().map(|o| o.operator.as_str()).collect();
        assert_eq!(
            names,
            ["Tf", "T*", "TJ", "q", "Tf", "Tj", "Q", "Tw", "Tc", "T*", "TJ", "TJ"]
        );
        assert_eq!(result[7].operands[0].as_i64().unwrap(), 3);
        assert_eq!(result[8].operands[0].as_i64().unwrap(), 2);
        let arr = result.last().unwrap().operands[0].as_array().unwrap();
        assert_eq!(arr[1].as_float().unwrap(), 100.0);
        assert_eq!(arr[2].as_i64().unwrap(), 120);
        assert_eq!(arr[3].as_str().unwrap(), b"B");
    }

    #[test]
    fn changed_character_codes_are_never_compensated_using_an_old_mapping() {
        let (mut doc, font, stream) = owned_document();
        let snapshot = Snapshot::capture(&doc);
        doc.get_dictionary_mut(font)
            .unwrap()
            .set("Widths", vec![500.into()]);
        doc.get_object_mut(stream)
            .unwrap()
            .as_stream_mut()
            .unwrap()
            .set_plain_content(b"BT /F 12 Tf (BB) Tj ET".to_vec());
        let before = doc.objects[&stream].clone();
        assert_eq!(snapshot.restore(&mut doc), 0);
        assert_eq!(doc.objects[&stream], before);
    }
    #[test]
    fn glyph_fit_preserves_scaled_spacing_kerning_and_saved_state() {
        let editor = ContentEditor::from_stream_strict(
            b"/F 10 Tf 80 Tz 2 Tc 3 Tw [(A) 120 ( )] TJ q 50 Tz 7 5 (A) \" Q (A) Tj",
        )
        .unwrap();
        let fonts = BTreeMap::from([(b"F".to_vec(), (1, 0))]);
        let deltas = BTreeMap::from([((1, 0), BTreeMap::from([(b'A', -500.0), (b' ', -250.0)]))]);
        let fits = BTreeMap::from([((1, 0), BTreeMap::from([(b'A', 2.0), (b' ', 2.0)]))]);
        let result = compensate(editor.operations(), &fonts, &deltas, &fits).unwrap();
        // Simulate PDF horizontal advancement independently from the rewriter.
        let mut scale = 1.0;
        let (mut tc, mut tw) = (0.0, 0.0);
        let mut stack = Vec::new();
        let mut advances = Vec::new();
        for op in &result {
            match op.operator.as_str() {
                "Tz" => scale = op.operands[0].as_float().unwrap() / 100.0,
                "Tc" => tc = op.operands[0].as_float().unwrap(),
                "Tw" => tw = op.operands[0].as_float().unwrap(),
                "q" => stack.push((scale, tc, tw)),
                "Q" => (scale, tc, tw) = stack.pop().unwrap(),
                "Tj" => {
                    for c in op.operands[0].as_str().unwrap() {
                        let (w, spacing) = if *c == b' ' {
                            (250.0, tw)
                        } else {
                            (500.0, 0.0)
                        };
                        advances.push((w * 10.0 / 1000.0 + tc + spacing) * scale);
                    }
                }
                "TJ" => {
                    for item in op.operands[0].as_array().unwrap() {
                        advances.push(-item.as_float().unwrap() * 10.0 / 1000.0 * scale);
                    }
                }
                _ => {}
            }
        }
        let expected = [9.6, -0.96, 8.0, 7.5, 9.6];
        assert_eq!(advances.len(), expected.len());
        for (actual, expected) in advances.iter().zip(expected) {
            assert!((actual - expected).abs() < 0.0001, "{advances:?}");
        }
        assert_eq!((scale, tc, tw), (0.8, 2.0, 3.0));
    }

    #[test]
    fn changed_resource_binding_is_not_adjusted_with_the_old_font() {
        let (mut doc, font, stream) = owned_document();
        let snapshot = Snapshot::capture(&doc);
        let replacement = doc.add_object(doc.objects[&font].clone());
        let page = *doc.get_pages().values().next().unwrap();
        doc.get_dictionary_mut(page).unwrap().set(
            "Resources",
            dictionary! {"Font" => dictionary! {"F" => replacement}},
        );
        doc.get_dictionary_mut(font)
            .unwrap()
            .set("Widths", vec![500.into()]);
        let before = doc.objects[&stream].clone();
        assert_eq!(snapshot.restore(&mut doc), 0);
        assert_eq!(doc.objects[&stream], before);
    }
    #[test]
    fn existing_embedded_program_is_not_repositioned_from_contradictory_widths() {
        let (mut doc, font, stream) = owned_document();
        let program = doc.add_object(lopdf::Stream::new(dictionary! {}, vec![0; 16]));
        let descriptor = doc.add_object(dictionary! {"FontFile2" => program});
        doc.get_dictionary_mut(font)
            .unwrap()
            .set("FontDescriptor", descriptor);
        let snapshot = Snapshot::capture(&doc);
        doc.get_dictionary_mut(font)
            .unwrap()
            .set("Widths", vec![500.into()]);
        let before = doc.objects[&stream].clone();
        assert_eq!(snapshot.restore(&mut doc), 0);
        assert_eq!(doc.objects[&stream], before);
    }
}
