//! Reading and querying the xref table of a PDF file.

use crate::crypto::{DecryptionError, DecryptionTarget, Decryptor, get};
use crate::data::Data;
use crate::metadata::Metadata;
use crate::object::Name;
use crate::object::ObjectIdentifier;
use crate::object::Stream;
use crate::object::dict::keys::{
    AUTHOR, CREATION_DATE, CREATOR, ENCRYPT, FIRST, ID, INDEX, INFO, KEYWORDS, MOD_DATE, N,
    OCPROPERTIES, PAGES, PREV, PRODUCER, ROOT, SIZE, SUBJECT, TITLE, TYPE, VERSION, W, XREF_STM,
};
use crate::object::indirect::IndirectObject;
use crate::object::{Array, MaybeRef};
use crate::object::{DateTime, Dict};
use crate::object::{Object, ObjectLike};
use crate::pdf::{PdfLoadLimits, PdfVersion};
use crate::reader::Reader;
use crate::reader::{Readable, ReaderContext, ReaderExt};
use crate::sync::{Arc, FxHashMap, RwLock, RwLockExt};
use crate::{PdfData, object};
use alloc::vec;
use alloc::vec::Vec;
use core::cmp::max;
use core::iter;
use core::ops::Deref;
use log::{error, warn};

pub(crate) const XREF_ENTRY_LEN: usize = 20;

#[derive(Debug, Copy, Clone)]
pub(crate) enum XRefError {
    Unknown,
    Encryption(DecryptionError),
}

/// Parse the "root" xref from the PDF.
pub(crate) fn root_xref(
    data: PdfData,
    password: &[u8],
    limits: PdfLoadLimits,
) -> Result<XRef, XRefError> {
    let mut xref_map = FxHashMap::default();
    let xref_pos = find_last_xref_pos(data.as_ref()).ok_or(XRefError::Unknown)?;
    let trailer =
        populate_xref_impl(data.as_ref(), xref_pos, &mut xref_map).ok_or(XRefError::Unknown)?;

    XRef::new(
        data.clone(),
        xref_map,
        XRefInput::TrailerDictData(trailer),
        false,
        password,
        limits,
    )
}

/// Try to manually parse the PDF to build an xref table and trailer dictionary.
pub(crate) fn fallback(data: PdfData, password: &[u8], limits: PdfLoadLimits) -> Option<XRef> {
    warn!("xref table was invalid, trying to manually build xref table");
    let (xref_map, xref_input) = fallback_xref_map(&data, password);

    if let Some(xref_input) = xref_input {
        warn!("rebuild xref table with {} entries", xref_map.len());

        XRef::new(data.clone(), xref_map, xref_input, true, password, limits).ok()
    } else {
        warn!("couldn't find trailer dictionary, failed to rebuild xref table");

        None
    }
}

fn fallback_xref_map<'a>(data: &'a PdfData, password: &[u8]) -> (XrefMap, Option<XRefInput<'a>>) {
    fallback_xref_map_inner(data, ReaderContext::dummy(), true, password)
}

fn fallback_xref_map_inner<'a>(
    data: &'a PdfData,
    mut dummy_ctx: ReaderContext<'a>,
    recurse: bool,
    password: &[u8],
) -> (XrefMap, Option<XRefInput<'a>>) {
    let mut xref_map = FxHashMap::default();
    let mut trailer_dicts = vec![];
    let mut root_ref = None;

    let mut r = Reader::new(data.as_ref());

    let mut last_obj_num = None;

    loop {
        let cur_pos = r.offset();

        let mut old_r = r.clone();

        if let Some(obj_id) = r.read::<ObjectIdentifier>(&dummy_ctx) {
            let mut cloned = r.clone();
            // Check that the object following it is actually valid before inserting it.
            cloned.skip_white_spaces_and_comments();
            if cloned.skip::<Object<'_>>(false).is_some() {
                xref_map.insert(obj_id, EntryType::Normal(cur_pos));
                last_obj_num = Some(obj_id);
                dummy_ctx.set_obj_number(obj_id);
            }
        } else if let Some(dict) = r.read::<Dict<'_>>(&dummy_ctx) {
            if dict.contains_key(ROOT) {
                trailer_dicts.push(dict.clone());
            }

            if dict
                .get::<Name>(TYPE)
                .is_some_and(|n| n.as_str() == "Catalog")
            {
                root_ref = last_obj_num;
            }

            if let Some(stream) = old_r.read::<Stream<'_>>(&dummy_ctx)
                && stream.dict().get::<Name>(TYPE).as_deref() == Some(b"ObjStm")
                && let Some(data) = stream.decoded().ok()
                && let Some(last_obj_num) = last_obj_num
                && let Some(obj_stream) = ObjectStream::new(stream, &data, &dummy_ctx)
            {
                for (idx, (obj_num, _)) in obj_stream.offsets.iter().enumerate() {
                    let id = ObjectIdentifier::new(*obj_num as i32, 0);
                    // If we already found an entry for that object number that was not
                    // inside an object stream. Somewhat arbitrary and maybe
                    // we can do better, but that seems to work for the current
                    // set of tests.
                    if xref_map
                        .get(&id)
                        .is_none_or(|e| !matches!(e, &EntryType::Normal(_)))
                    {
                        xref_map.insert(
                            id,
                            EntryType::ObjStream(last_obj_num.obj_number as u32, idx as u32),
                        );
                    }
                }
            }
        } else {
            r.read_byte();
        }

        if r.at_end() {
            break;
        }
    }

    // Try to choose the right trailer dict by doing basic validation.
    let mut trailer_dict = None;

    for dict in trailer_dicts {
        if let Some(root_id) = dict.get_raw::<Dict<'_>>(ROOT) {
            let check = |dict: &Dict<'_>| -> bool { dict.contains_key(PAGES) };

            match root_id {
                MaybeRef::Ref(r) => match xref_map.get(&r.into()) {
                    Some(EntryType::Normal(offset)) => {
                        let mut reader = Reader::new(&data.as_ref()[*offset..]);

                        if let Some(obj) =
                            reader.read_with_context::<IndirectObject<Dict<'_>>>(&dummy_ctx)
                            && check(&obj.clone().get())
                        {
                            trailer_dict = Some(dict);
                        }
                    }
                    Some(EntryType::ObjStream(obj_num, idx)) => {
                        if let Some(EntryType::Normal(offset)) =
                            xref_map.get(&ObjectIdentifier::new(*obj_num as i32, 0))
                        {
                            let mut reader = Reader::new(&data.as_ref()[*offset..]);

                            if let Some(stream) =
                                reader.read_with_context::<IndirectObject<Stream<'_>>>(&dummy_ctx)
                                && let Some(data) = stream.clone().get().decoded().ok()
                                && let Some(object_stream) =
                                    ObjectStream::new(stream.get(), &data, &dummy_ctx)
                                && let Some(obj) = object_stream.get::<Dict<'_>>(*idx)
                                && check(&obj)
                            {
                                trailer_dict = Some(dict);
                            }
                        }
                    }
                    _ => {}
                },
                MaybeRef::NotRef(d) => {
                    if check(&d) {
                        trailer_dict = Some(dict);
                    }
                }
            }
        }
    }

    let has_encryption = trailer_dict
        .as_ref()
        .is_some_and(|t| t.contains_key(ENCRYPT));

    if has_encryption && recurse {
        // The problem is that in this case, we have used a dummy reader context which does not have
        // a decryptor. Therefore, we were unable to decrypt any of the object streams and missed
        // all objects that are inside of such a stream. Therefore, we need to redo the process
        // using a `ReaderContext` that does have the ability to decrypt.
        if let Some(Ok(xref)) = trailer_dict.as_ref().map(|d| {
            XRef::new(
                data.clone(),
                xref_map.clone(),
                XRefInput::TrailerDictData(d.data()),
                true,
                password,
                PdfLoadLimits::default(),
            )
        }) {
            let ctx = ReaderContext::new(&xref, false);
            let (patched_map, _) = fallback_xref_map_inner(data, ctx, false, password);
            xref_map = patched_map;
        }
    }

    if let Some(trailer_dict_data) = trailer_dict.map(|d| d.data()) {
        (
            xref_map,
            Some(XRefInput::TrailerDictData(trailer_dict_data)),
        )
    } else if let Some(root_ref) = root_ref {
        (xref_map, Some(XRefInput::RootRef(root_ref)))
    } else {
        (xref_map, None)
    }
}

const DUMMY_XREF: XRef = XRef(Inner::Dummy);

/// An xref table.
#[derive(Debug, Clone)]
pub struct XRef(Inner);

impl XRef {
    fn new(
        data: PdfData,
        xref_map: XrefMap,
        input: XRefInput<'_>,
        repaired: bool,
        password: &[u8],
        load_limits: PdfLoadLimits,
    ) -> Result<Self, XRefError> {
        // This is a bit hacky, but the problem is we can't read the resolved trailer dictionary
        // before we actually created the xref struct. So we first create it using dummy data
        // and then populate the data.
        let trailer_data = TrailerData::dummy();

        let mut xref = Self(Inner::Some(Arc::new(SomeRepr {
            data: Arc::new(Data::new(data)),
            map: Arc::new(RwLock::new(MapRepr { xref_map, repaired })),
            decryptor: Arc::new(Decryptor::None),
            has_ocgs: false,
            metadata: Arc::new(Metadata::default()),
            trailer_data,
            password: password.to_vec(),
            load_limits,
        })));

        // We read the trailer twice, once to determine the encryption used and then a second
        // time to resolve the catalog dictionary, etc. This allows us to support catalog dictionaries
        // that are stored in an encrypted object stream.

        let decryptor = {
            match input {
                XRefInput::TrailerDictData(trailer_dict_data) => {
                    let mut r = Reader::new(trailer_dict_data);

                    let trailer_dict = r
                        .read_with_context::<Dict<'_>>(&ReaderContext::new(&xref, false))
                        .ok_or(XRefError::Unknown)?;

                    get_decryptor(&trailer_dict, password)?
                }
                XRefInput::RootRef(_) => Decryptor::None,
            }
        };

        match &mut xref.0 {
            Inner::Dummy => unreachable!(),
            Inner::Some(r) => {
                let mutable = Arc::make_mut(r);
                mutable.decryptor = Arc::new(decryptor.clone());
            }
        }

        let (trailer_data, has_ocgs, metadata) = match input {
            XRefInput::TrailerDictData(trailer_dict_data) => {
                let mut r = Reader::new(trailer_dict_data);

                let trailer_dict = r
                    .read_with_context::<Dict<'_>>(&ReaderContext::new(&xref, false))
                    .ok_or(XRefError::Unknown)?;

                let root_ref = trailer_dict.get_ref(ROOT).ok_or(XRefError::Unknown)?;
                let root = trailer_dict
                    .get::<Dict<'_>>(ROOT)
                    .ok_or(XRefError::Unknown)?;
                let metadata = trailer_dict
                    .get::<Dict<'_>>(INFO)
                    .map(|d| parse_metadata(&d))
                    .unwrap_or_default();
                let pages_ref = root.get_ref(PAGES).ok_or(XRefError::Unknown)?;
                let has_ocgs = root.get::<Dict<'_>>(OCPROPERTIES).is_some();
                let version = root
                    .get::<Name>(VERSION)
                    .and_then(|v| PdfVersion::from_bytes(v.deref()));

                let td = TrailerData {
                    pages_ref: pages_ref.into(),
                    root_ref: root_ref.into(),
                    version,
                };

                (td, has_ocgs, metadata)
            }
            XRefInput::RootRef(root_ref) => {
                let root = xref.get::<Dict<'_>>(root_ref).ok_or(XRefError::Unknown)?;
                let pages_ref = root.get_ref(PAGES).ok_or(XRefError::Unknown)?;

                let td = TrailerData {
                    pages_ref: pages_ref.into(),
                    root_ref,
                    version: None,
                };

                (td, false, Metadata::default())
            }
        };

        match &mut xref.0 {
            Inner::Dummy => unreachable!(),
            Inner::Some(r) => {
                let mutable = Arc::make_mut(r);
                mutable.trailer_data = trailer_data;
                mutable.decryptor = Arc::new(decryptor);
                mutable.has_ocgs = has_ocgs;
                mutable.metadata = Arc::new(metadata);
            }
        }

        Ok(xref)
    }

    fn is_repaired(&self) -> bool {
        match &self.0 {
            Inner::Dummy => false,
            Inner::Some(r) => {
                let locked = r.map.get();
                locked.repaired
            }
        }
    }

    pub(crate) fn dummy() -> &'static Self {
        &DUMMY_XREF
    }

    pub(crate) fn load_limits(&self) -> PdfLoadLimits {
        match &self.0 {
            Inner::Dummy => PdfLoadLimits::default(),
            Inner::Some(r) => r.load_limits,
        }
    }

    pub(crate) fn len(&self) -> usize {
        match &self.0 {
            Inner::Dummy => 0,
            Inner::Some(r) => r.map.get().xref_map.len(),
        }
    }

    pub(crate) fn trailer_data(&self) -> &TrailerData {
        match &self.0 {
            Inner::Dummy => unreachable!(),
            Inner::Some(r) => &r.trailer_data,
        }
    }

    /// Number of cached parsed object-stream offset tables. QF2-B test
    /// hook; not part of the public API.
    #[cfg(test)]
    pub(crate) fn object_stream_offsets_cache_len(&self) -> usize {
        match &self.0 {
            Inner::Dummy => 0,
            Inner::Some(r) => r.data.object_stream_offsets_cache_len(),
        }
    }

    pub(crate) fn metadata(&self) -> &Metadata {
        match &self.0 {
            Inner::Dummy => unreachable!(),
            Inner::Some(r) => &r.metadata,
        }
    }

    /// Return the object ID of the root dictionary.
    pub fn root_id(&self) -> ObjectIdentifier {
        self.trailer_data().root_ref
    }

    /// Whether the PDF has optional content groups.
    pub fn has_optional_content_groups(&self) -> bool {
        match &self.0 {
            Inner::Dummy => false,
            Inner::Some(r) => r.has_ocgs,
        }
    }

    pub(crate) fn objects(&self) -> impl IntoIterator<Item = Object<'_>> + '_ {
        match &self.0 {
            Inner::Dummy => unimplemented!(),
            Inner::Some(r) => {
                let locked = r.map.get();
                let mut elements = locked
                    .xref_map
                    .iter()
                    .map(|(id, e)| {
                        let offset = match e {
                            EntryType::Normal(o) => (*o, 0),
                            EntryType::ObjStream(id, index) => {
                                if let Some(EntryType::Normal(offset)) =
                                    locked.xref_map.get(&ObjectIdentifier::new(*id as i32, 0))
                                {
                                    (*offset, *index)
                                } else {
                                    (usize::MAX, 0)
                                }
                            }
                        };

                        (*id, offset)
                    })
                    .collect::<Vec<_>>();

                // Try to yield in the order the objects appeared in the
                // PDF.
                elements.sort_by_key(|e1| e1.1);

                let mut iter = elements.into_iter();

                iter::from_fn(move || {
                    for next in iter.by_ref() {
                        if let Some(obj) = self.get_with(next.0, &ReaderContext::new(self, false)) {
                            return Some(obj);
                        } else {
                            // Skip invalid objects.
                            continue;
                        }
                    }

                    None
                })
            }
        }
    }

    pub(crate) fn repair(&self) {
        let Inner::Some(r) = &self.0 else {
            unreachable!();
        };

        let mut locked = r
            .map
            .try_put()
            .expect("xref repair: map lock not contended");
        assert!(!locked.repaired);

        let (xref_map, _) = fallback_xref_map(r.data.get(), &r.password);
        locked.xref_map = xref_map;
        locked.repaired = true;
    }

    #[inline]
    pub(crate) fn needs_decryption(&self, ctx: &ReaderContext<'_>) -> bool {
        match &self.0 {
            Inner::Dummy => false,
            Inner::Some(r) => {
                if matches!(r.decryptor.as_ref(), Decryptor::None) {
                    false
                } else {
                    !ctx.in_content_stream() && !ctx.in_object_stream()
                }
            }
        }
    }

    #[inline]
    pub(crate) fn decrypt(
        &self,
        id: ObjectIdentifier,
        data: &[u8],
        target: DecryptionTarget,
    ) -> Option<Vec<u8>> {
        match &self.0 {
            Inner::Dummy => Some(data.to_vec()),
            Inner::Some(r) => r.decryptor.decrypt(id, data, target),
        }
    }

    /// Return the object with the given identifier.
    #[allow(private_bounds)]
    pub fn get<'a, T>(&'a self, id: ObjectIdentifier) -> Option<T>
    where
        T: ObjectLike<'a>,
    {
        let ctx = ReaderContext::new(self, false);
        self.get_with(id, &ctx)
    }

    /// Return the object with the given identifier.
    #[allow(private_bounds)]
    pub(crate) fn get_with<'a, T>(
        &'a self,
        id: ObjectIdentifier,
        ctx: &ReaderContext<'a>,
    ) -> Option<T>
    where
        T: ObjectLike<'a>,
    {
        let Inner::Some(repr) = &self.0 else {
            return None;
        };

        let locked = repr.map.try_get()?;

        let mut r = Reader::new(repr.data.get().as_ref());

        let entry = *locked.xref_map.get(&id).or({
            // An indirect reference to an undefined object shall not be considered an error by a PDF processor; it
            // shall be treated as a reference to the null object.
            None
        })?;
        drop(locked);

        let mut ctx = ctx.clone();
        ctx.set_obj_number(id);
        ctx.set_in_content_stream(false);

        match entry {
            EntryType::Normal(offset) => {
                ctx.set_in_object_stream(false);
                r.jump(offset);

                if let Some(object) = r.read_with_context::<IndirectObject<T>>(&ctx) {
                    if object.id() == &id {
                        return Some(object.get());
                    }
                } else {
                    // There is a valid object at the offset, it's just not of the type the caller
                    // expected, which is fine.
                    if r.skip_not_in_content_stream::<IndirectObject<Object<'_>>>()
                        .is_some()
                    {
                        return None;
                    }
                };

                // The xref table is broken, try to repair if not already repaired.
                if self.is_repaired() {
                    error!(
                        "attempt was made at repairing xref, but object {id:?} still couldn't be read"
                    );

                    None
                } else {
                    warn!("broken xref, attempting to repair");

                    self.repair();

                    // Now try reading again.
                    self.get_with::<T>(id, &ctx)
                }
            }
            EntryType::ObjStream(obj_stram_gen_num, index) => {
                // Generation number is implicitly 0.
                let obj_stream_id = ObjectIdentifier::new(obj_stram_gen_num as i32, 0);

                if obj_stream_id == id {
                    warn!("cycle detected in object stream");

                    return None;
                }

                let stream = self.get_with::<Stream<'_>>(obj_stream_id, &ctx)?;
                let data = repr.data.get_with(obj_stream_id, &ctx)?;
                // QF2-B: re-use a cached `(obj_num, offset)` index table if
                // we've already parsed this `/ObjStm` once for this
                // document. The cache lives on the per-document `Data` and
                // is dropped together with it.
                let offsets = repr
                    .data
                    .get_object_stream_offsets_or_init(obj_stream_id, || {
                        parse_object_stream_offsets(&stream, data)
                    })?;
                let object_stream = ObjectStream::from_cached_offsets(data, &ctx, offsets);
                object_stream.get(index)
            }
        }
    }
}

/// An input that is passed to the xref constructor so that we can fully resolve
/// the PDF.
#[derive(Debug, Copy, Clone)]
pub(crate) enum XRefInput<'a> {
    /// This option is going to be uesd in 99.999% of the case. It contains the
    /// raw data of the trailer dictionary which is then going to be processed.
    TrailerDictData(&'a [u8]),
    /// In case the trailer dictionary could not be read (for example because
    /// it is cut-off), we just pass the object ID of the root dictionary
    /// in case we have found one, and try our best to build the PDF just
    /// with the information we have there.
    ///
    /// Note that this won't work if the document is encrypted, as we
    /// can't access the crypto dictionary.
    RootRef(ObjectIdentifier),
}

pub(crate) fn find_last_xref_pos(data: &[u8]) -> Option<usize> {
    let mut finder = Reader::new(data);
    let mut pos = finder.len().checked_sub(1)?;
    finder.jump(pos);

    let needle = b"startxref";

    loop {
        if finder.forward_tag(needle).is_some() {
            finder.skip_white_spaces_and_comments();

            let offset = finder.read_without_context::<i32>()?.try_into().ok()?;

            return Some(offset);
        }

        pos = pos.checked_sub(1)?;
        finder.jump(pos);
    }
}

/// A type of xref entry.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum EntryType {
    /// An indirect object that is at a specific offset in the original data.
    Normal(usize),
    /// An indirect object that is part of an object stream. First number indicates the object
    /// number of the _object stream_ (the generation number is always 0), the second number indicates
    /// the index in the object stream.
    ObjStream(u32, u32),
}

type XrefMap = FxHashMap<ObjectIdentifier, EntryType>;

/// Representation of a proper xref table.
#[derive(Debug)]
struct MapRepr {
    xref_map: XrefMap,
    repaired: bool,
}

#[derive(Debug, Copy, Clone)]
pub(crate) struct TrailerData {
    pub(crate) pages_ref: ObjectIdentifier,
    pub(crate) root_ref: ObjectIdentifier,
    pub(crate) version: Option<PdfVersion>,
}

impl TrailerData {
    pub(crate) fn dummy() -> Self {
        Self {
            pages_ref: ObjectIdentifier::new(0, 0),
            root_ref: ObjectIdentifier::new(0, 0),
            version: None,
        }
    }
}

#[derive(Debug, Clone)]
struct SomeRepr {
    data: Arc<Data>,
    map: Arc<RwLock<MapRepr>>,
    metadata: Arc<Metadata>,
    decryptor: Arc<Decryptor>,
    has_ocgs: bool,
    password: Vec<u8>,
    trailer_data: TrailerData,
    load_limits: PdfLoadLimits,
}

#[derive(Debug, Clone)]
enum Inner {
    /// A dummy xref table that doesn't have any entries.
    Dummy,
    /// A proper xref table.
    Some(Arc<SomeRepr>),
}

#[derive(Debug)]
struct XRefEntry {
    offset: usize,
    gen_number: i32,
    used: bool,
}

impl XRefEntry {
    pub(crate) fn read(data: &[u8]) -> Option<Self> {
        #[inline(always)]
        fn parse_u32(data: &[u8]) -> Option<u32> {
            let mut accum = 0_u32;

            for byte in data {
                accum = accum.checked_mul(10)?;

                match *byte {
                    b'0'..=b'9' => accum = accum.checked_add((*byte - b'0') as u32)?,
                    _ => return None,
                }
            }

            Some(accum)
        }

        let offset = parse_u32(&data[0..10])? as usize;
        let gen_number = i32::try_from(parse_u32(&data[11..16])?).ok()?;

        let used = data[17] == b'n';

        Some(Self {
            offset,
            gen_number,
            used,
        })
    }
}

/// Maximum depth for following xref Prev/XRefStm chains to prevent stack
/// overflow on circular or deeply chained xref tables.
const MAX_XREF_CHAIN_DEPTH: usize = 64;

fn populate_xref_impl<'a>(data: &'a [u8], pos: usize, xref_map: &mut XrefMap) -> Option<&'a [u8]> {
    populate_xref_depth(data, pos, xref_map, 0)
}

fn populate_xref_depth<'a>(
    data: &'a [u8],
    pos: usize,
    xref_map: &mut XrefMap,
    depth: usize,
) -> Option<&'a [u8]> {
    if depth > MAX_XREF_CHAIN_DEPTH {
        log::warn!("Xref chain depth exceeds {MAX_XREF_CHAIN_DEPTH}, stopping traversal");
        return None;
    }
    let mut reader = Reader::new(data);
    reader.jump(pos);
    // In case the position points to before the object number of a xref stream.
    reader.skip_white_spaces_and_comments();

    let mut r2 = reader.clone();
    if reader
        .clone()
        .read_without_context::<ObjectIdentifier>()
        .is_some()
    {
        populate_from_xref_stream(data, &mut r2, xref_map, depth)
    } else {
        populate_from_xref_table(data, &mut r2, xref_map, depth)
    }
}

pub(super) struct SubsectionHeader {
    pub(super) start: u32,
    pub(super) num_entries: u32,
}

impl Readable<'_> for SubsectionHeader {
    fn read(r: &mut Reader<'_>, _: &ReaderContext<'_>) -> Option<Self> {
        r.skip_white_spaces();
        let start = r.read_without_context::<u32>()?;
        r.skip_white_spaces();
        let num_entries = r.read_without_context::<u32>()?;
        r.skip_white_spaces();

        Some(Self { start, num_entries })
    }
}

/// Populate the xref table, and return the trailer dict.
fn populate_from_xref_table<'a>(
    data: &'a [u8],
    reader: &mut Reader<'a>,
    insert_map: &mut XrefMap,
    depth: usize,
) -> Option<&'a [u8]> {
    let trailer = {
        let mut reader = reader.clone();
        read_xref_table_trailer(&mut reader, &ReaderContext::dummy())?
    };

    reader.skip_white_spaces();
    reader.forward_tag(b"xref")?;
    reader.skip_white_spaces();

    let mut max_obj = 0;

    if let Some(prev) = trailer.get::<i32>(PREV) {
        // First insert the entries from any previous xref tables.
        populate_xref_depth(data, prev as usize, insert_map, depth + 1)?;
    }

    // In hybrid files, entries in `XRefStm` should have higher priority, therefore we insert them
    // after looking at `PREV`.
    if let Some(xref_stm) = trailer.get::<i32>(XREF_STM) {
        populate_xref_depth(data, xref_stm as usize, insert_map, depth + 1)?;
    }

    while let Some(header) = reader.read_without_context::<SubsectionHeader>() {
        reader.skip_white_spaces();

        let start = header.start;
        // Both come from the file. Ported from hayro upstream
        // (LaurenzV/hayro#1194).
        let end = start.checked_add(header.num_entries)?;

        for obj_number in start..end {
            max_obj = max(max_obj, obj_number);
            let bytes = reader.read_bytes(XREF_ENTRY_LEN)?;
            let entry = XRefEntry::read(bytes)?;

            // Specification says we should ignore any object number > SIZE, but probably
            // not important?
            if entry.used {
                insert_map.insert(
                    ObjectIdentifier::new(obj_number as i32, entry.gen_number),
                    EntryType::Normal(entry.offset),
                );
            }
        }
    }

    Some(trailer.data())
}

fn populate_from_xref_stream<'a>(
    data: &'a [u8],
    reader: &mut Reader<'a>,
    insert_map: &mut XrefMap,
    depth: usize,
) -> Option<&'a [u8]> {
    let stream = reader
        .read_with_context::<IndirectObject<Stream<'_>>>(&ReaderContext::dummy())?
        .get();

    if let Some(prev) = stream.dict().get::<i32>(PREV) {
        // First insert the entries from any previous xref tables.
        let _ = populate_xref_depth(data, prev as usize, insert_map, depth + 1)?;
    }

    let size = stream.dict().get::<u32>(SIZE)?;

    let [f1_len, f2_len, f3_len] = stream.dict().get::<[u8; 3]>(W)?;

    if f2_len > size_of::<u64>() as u8 {
        error!("xref offset length is larger than the allowed limit");

        return None;
    }

    // Do such files exist?
    if f1_len != 1 {
        warn!("first field in xref stream was longer than 1");
    }

    let xref_data = stream.decoded().ok()?;
    let mut xref_reader = Reader::new(xref_data.as_ref());

    if let Some(arr) = stream.dict().get::<Array<'_>>(INDEX) {
        let iter = arr.iter::<(u32, u32)>();

        for (start, num_elements) in iter {
            xref_stream_subsection(
                &mut xref_reader,
                start,
                num_elements,
                f1_len,
                f2_len,
                f3_len,
                insert_map,
            )?;
        }
    } else {
        xref_stream_subsection(
            &mut xref_reader,
            0,
            size,
            f1_len,
            f2_len,
            f3_len,
            insert_map,
        )?;
    }

    Some(stream.dict().data())
}

fn xref_stream_num(data: &[u8]) -> Option<u32> {
    Some(match data.len() {
        0 => return None,
        1 => u8::from_be(data[0]) as u32,
        2 => u16::from_be_bytes(data[0..2].try_into().ok()?) as u32,
        3 => u32::from_be_bytes([0, data[0], data[1], data[2]]),
        4 => u32::from_be_bytes(data[0..4].try_into().ok()?),
        8 => {
            if let Ok(num) = u32::try_from(u64::from_be_bytes(data[0..8].try_into().ok()?)) {
                return Some(num);
            } else {
                warn!("xref stream number is too large");

                return None;
            }
        }
        n => {
            warn!("invalid xref stream number {n}");

            return None;
        }
    })
}

fn xref_stream_subsection<'a>(
    xref_reader: &mut Reader<'a>,
    start: u32,
    num_elements: u32,
    f1_len: u8,
    f2_len: u8,
    f3_len: u8,
    insert_map: &mut XrefMap,
) -> Option<()> {
    for i in 0..num_elements {
        let f_type = if f1_len == 0 {
            1
        } else {
            // We assume a length of 1.
            xref_reader.read_bytes(1)?[0]
        };

        let obj_number = start + i;

        match f_type {
            // We don't care about free objects.
            0 => {
                xref_reader.skip_bytes(f2_len as usize + f3_len as usize)?;
            }
            1 => {
                let offset = if f2_len > 0 {
                    let data = xref_reader.read_bytes(f2_len as usize)?;
                    xref_stream_num(data)?
                } else {
                    0
                };

                let gen_number = if f3_len > 0 {
                    let data = xref_reader.read_bytes(f3_len as usize)?;
                    xref_stream_num(data)?
                } else {
                    0
                };

                insert_map.insert(
                    ObjectIdentifier::new(obj_number as i32, gen_number as i32),
                    EntryType::Normal(offset as usize),
                );
            }
            2 => {
                let obj_stream_number = {
                    let data = xref_reader.read_bytes(f2_len as usize)?;
                    xref_stream_num(data)?
                };
                let gen_number = 0;
                let index = if f3_len > 0 {
                    let data = xref_reader.read_bytes(f3_len as usize)?;
                    xref_stream_num(data)?
                } else {
                    0
                };

                insert_map.insert(
                    ObjectIdentifier::new(obj_number as i32, gen_number),
                    EntryType::ObjStream(obj_stream_number, index),
                );
            }
            _ => {
                warn!("xref has unknown field type {f_type}");

                return None;
            }
        }
    }

    Some(())
}

fn read_xref_table_trailer<'a>(
    reader: &mut Reader<'a>,
    ctx: &ReaderContext<'a>,
) -> Option<Dict<'a>> {
    reader.skip_white_spaces();
    reader.forward_tag(b"xref")?;
    reader.skip_white_spaces();

    while let Some(header) = reader.read_without_context::<SubsectionHeader>() {
        let len = XREF_ENTRY_LEN.checked_mul(header.num_entries as usize)?;
        reader.jump(reader.offset().checked_add(len)?);
    }

    reader.skip_white_spaces();
    reader.forward_tag(b"trailer")?;
    reader.skip_white_spaces();

    reader.read_with_context::<Dict<'_>>(ctx)
}

fn get_decryptor(trailer_dict: &Dict<'_>, password: &[u8]) -> Result<Decryptor, XRefError> {
    if let Some(encryption_dict) = trailer_dict.get::<Dict<'_>>(ENCRYPT) {
        let id = if let Some(id) = trailer_dict
            .get::<Array<'_>>(ID)
            .and_then(|a| a.flex_iter().next::<object::String>())
        {
            id.to_vec()
        } else {
            // Assume an empty ID entry.
            vec![]
        };

        get(&encryption_dict, &id, password).map_err(XRefError::Encryption)
    } else {
        Ok(Decryptor::None)
    }
}

/// Parse the `(obj_num, absolute_byte_offset)` index table that lives at the
/// start of a compressed `/ObjStm`.
///
/// Returns `None` if the stream dict is missing `/N` / `/First`, or if the
/// header is truncated. The returned table is the same value that an
/// `ObjectStream` would have populated internally before this was split out
/// (QF2-B). Splitting it allows the result to be cached per-document; see
/// [`crate::data::Data::get_object_stream_offsets_or_init`].
fn parse_object_stream_offsets(
    inner: &Stream<'_>,
    data: &[u8],
) -> Option<crate::data::ObjectStreamOffsets> {
    let num_objects = inner.dict().get::<usize>(N)?;
    let first_offset = inner.dict().get::<usize>(FIRST)?;

    let mut r = Reader::new(data);
    let mut offsets = Vec::with_capacity(num_objects);

    for _ in 0..num_objects {
        r.skip_white_spaces_and_comments();
        // Skip object number
        let obj_num = r.read_without_context::<u32>()?;
        r.skip_white_spaces_and_comments();
        let relative_offset = r.read_without_context::<usize>()?;
        offsets.push((obj_num, first_offset + relative_offset));
    }

    Some(offsets)
}

/// Holds a borrowed view onto the decoded bytes of an `/ObjStm` plus a
/// (possibly cached) parsed offset table.
///
/// QF2-B: `offsets` is now an `Arc<...>` so the same allocation can be
/// returned from the per-document cache on subsequent lookups, eliminating
/// the linear re-parse hot loop reported in
/// `QF1_A_FLAMEGRAPH_REPORT.md` (449× per main thread on the
/// `scan_for_xfa` fallback path).
struct ObjectStream<'a> {
    data: &'a [u8],
    ctx: ReaderContext<'a>,
    offsets: Arc<crate::data::ObjectStreamOffsets>,
}

impl<'a> ObjectStream<'a> {
    /// Build a fresh `ObjectStream` by parsing the index table inline (no
    /// caching). Used by the xref-repair / trailer-fallback paths that
    /// don't have access to a `Data` cache.
    fn new(inner: Stream<'_>, data: &'a [u8], ctx: &ReaderContext<'a>) -> Option<Self> {
        let offsets = Arc::new(parse_object_stream_offsets(&inner, data)?);

        let mut ctx = ctx.clone();
        ctx.set_in_object_stream(true);

        Some(Self { data, ctx, offsets })
    }

    /// Build an `ObjectStream` that reuses an already-parsed offsets table
    /// (typically retrieved from the per-document cache). Cheap — does no
    /// header scan.
    fn from_cached_offsets(
        data: &'a [u8],
        ctx: &ReaderContext<'a>,
        offsets: Arc<crate::data::ObjectStreamOffsets>,
    ) -> Self {
        let mut ctx = ctx.clone();
        ctx.set_in_object_stream(true);

        Self { data, ctx, offsets }
    }

    fn get<T>(&self, index: u32) -> Option<T>
    where
        T: ObjectLike<'a>,
    {
        let offset = self.offsets.get(index as usize)?.1;
        let mut r = Reader::new(self.data);
        r.jump(offset);
        r.skip_white_spaces_and_comments();

        r.read_with_context::<T>(&self.ctx)
    }
}

fn parse_metadata(info_dict: &Dict<'_>) -> Metadata {
    Metadata {
        creation_date: info_dict
            .get::<object::String>(CREATION_DATE)
            .and_then(|c| DateTime::from_bytes(&c)),
        modification_date: info_dict
            .get::<object::String>(MOD_DATE)
            .and_then(|c| DateTime::from_bytes(&c)),
        title: info_dict.get::<object::String>(TITLE).map(|t| t.to_vec()),
        author: info_dict.get::<object::String>(AUTHOR).map(|t| t.to_vec()),
        subject: info_dict.get::<object::String>(SUBJECT).map(|t| t.to_vec()),
        keywords: info_dict
            .get::<object::String>(KEYWORDS)
            .map(|t| t.to_vec()),
        creator: info_dict.get::<object::String>(CREATOR).map(|t| t.to_vec()),
        producer: info_dict
            .get::<object::String>(PRODUCER)
            .map(|t| t.to_vec()),
    }
}

#[cfg(test)]
mod qf2b_objectstream_cache_tests {
    //! QF2-B — end-to-end coverage for the per-document ObjectStream
    //! offsets cache, using a real `/ObjStm`-containing fixture.

    use crate::pdf::Pdf;
    use crate::xref::parse_object_stream_offsets;

    /// A PDF 1.5 document carrying an `/ObjStm`, built here rather than read
    /// from the corpus.
    ///
    /// This used to name a corpus file and `return` when it was missing — so in
    /// a tree without the goldens the test passed while asserting nothing, and
    /// the corpus name travelled into published source. Building the fixture
    /// fixes both: its provenance is this function, and it exists everywhere
    /// the test runs.
    ///
    /// Two objects (catalog and pages) packed into an object stream, addressed
    /// by an xref stream — the smallest shape that makes the offsets cache do
    /// its work.
    fn objstm_fixture() -> Vec<u8> {
        fn deflate(data: &[u8]) -> Vec<u8> {
            use std::io::Write;
            let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            e.write_all(data).expect("in-memory write");
            e.finish().expect("in-memory finish")
        }

        let catalog = b"<< /Type /Catalog /Pages 3 0 R >>";
        let pages = b"<< /Type /Pages /Kids [] /Count 0 >>";
        let pairs = format!("2 0 3 {}", catalog.len() + 1);
        let first = pairs.len() + 1;
        let mut payload = pairs.into_bytes();
        payload.push(b'\n');
        payload.extend_from_slice(catalog);
        payload.push(b' ');
        payload.extend_from_slice(pages);
        let body = deflate(&payload);

        let mut out = b"%PDF-1.5\n".to_vec();
        let objstm_at = out.len();
        out.extend_from_slice(
            format!(
                "1 0 obj\n<< /Type /ObjStm /N 2 /First {first} /Length {} \
                 /Filter /FlateDecode >>\nstream\n",
                body.len()
            )
            .as_bytes(),
        );
        out.extend_from_slice(&body);
        out.extend_from_slice(b"\nendstream\nendobj\n");

        let xref_at = out.len();
        let entry = |t: u8, a: usize, b: u8| {
            let mut v = vec![t];
            v.extend_from_slice(&(a as u16).to_be_bytes());
            v.push(b);
            v
        };
        let mut rows = Vec::new();
        rows.extend(entry(0, 0, 255));
        rows.extend(entry(1, objstm_at, 0));
        rows.extend(entry(2, 1, 0)); // object 2 lives in stream 1, index 0
        rows.extend(entry(2, 1, 1)); // object 3 lives in stream 1, index 1
        rows.extend(entry(1, xref_at, 0));
        let xbody = deflate(&rows);
        out.extend_from_slice(
            format!(
                "4 0 obj\n<< /Type /XRef /Size 5 /W [1 2 1] /Root 2 0 R /Length {} \
                 /Filter /FlateDecode >>\nstream\n",
                xbody.len()
            )
            .as_bytes(),
        );
        out.extend_from_slice(&xbody);
        out.extend_from_slice(b"\nendstream\nendobj\n");
        out.extend_from_slice(format!("startxref\n{xref_at}\n%%EOF\n").as_bytes());
        out
    }

    /// The fixture is built, so failing to parse it is a fault in this crate,
    /// not a missing file. It used to be an `Option` that the tests turned into
    /// an early `return`, which is how they passed while asserting nothing.
    fn load_fixture() -> Pdf {
        Pdf::new(objstm_fixture()).expect("the generated /ObjStm fixture must parse")
    }

    #[test]
    fn qf2b_objstm_cache_populates_and_is_stable_on_repeat() {
        let pdf = load_fixture();

        let xref = pdf.xref();

        // `Pdf::new` resolves the trailer and catalog during construction;
        // for PDF 1.5+ files those typically live in an `/ObjStm` so the
        // cache is already non-empty at this point. That is itself
        // evidence that the cache is active.
        let after_construction = xref.object_stream_offsets_cache_len();
        assert!(
            after_construction >= 1,
            "fixture is a PDF 1.5+ doc with /ObjStm; at least one offsets table should already be cached after construction; got {after_construction}"
        );

        // Resolve the catalog explicitly — must not grow the cache because
        // the /ObjStm carrying the catalog is already a hit.
        let _: Option<crate::object::Dict<'_>> = xref.get(xref.root_id());
        assert_eq!(
            xref.object_stream_offsets_cache_len(),
            after_construction,
            "repeated resolution of the same indirect object must reuse the cached offsets table"
        );

        // Resolve a number of additional indirect objects. Each new
        // `/ObjStm` we touch may add one entry, but re-touching anything
        // already seen must not.
        for raw in 1..=20i32 {
            let id = crate::object::ObjectIdentifier::new(raw, 0);
            let _: Option<crate::object::Dict<'_>> = xref.get(id);
        }
        let after_scan = xref.object_stream_offsets_cache_len();

        // Idempotency: a second sweep must not grow the cache further.
        for raw in 1..=20i32 {
            let id = crate::object::ObjectIdentifier::new(raw, 0);
            let _: Option<crate::object::Dict<'_>> = xref.get(id);
        }
        assert_eq!(
            xref.object_stream_offsets_cache_len(),
            after_scan,
            "repeated full scans must be cache-stable (no re-parse)"
        );
    }

    #[test]
    fn qf2b_two_pdfs_have_independent_caches() {
        let pdf_a = load_fixture();
        let pdf_b = load_fixture();

        // Sanity: both start with the same construction-time count for
        // the same fixture (same shape).
        let base_a = pdf_a.xref().object_stream_offsets_cache_len();
        let base_b = pdf_b.xref().object_stream_offsets_cache_len();
        assert_eq!(base_a, base_b);

        // Touch many ids in pdf_a to (likely) populate additional /ObjStm
        // cache entries.
        for raw in 1..=50i32 {
            let id = crate::object::ObjectIdentifier::new(raw, 0);
            let _: Option<crate::object::Dict<'_>> = pdf_a.xref().get(id);
        }
        let warm_a = pdf_a.xref().object_stream_offsets_cache_len();

        // pdf_b must NOT have grown — caches are per-document.
        assert_eq!(
            pdf_b.xref().object_stream_offsets_cache_len(),
            base_b,
            "pdf_b cache must be independent of pdf_a's warming (base_a={base_a}, warm_a={warm_a}, base_b={base_b})"
        );
    }

    #[test]
    fn qf2b_parse_helper_returns_none_on_truncated_header() {
        // Synthetic: dict says N=3 but data has only one (num, offset)
        // pair. The helper must return None, and the caller must not
        // cache a `None`.
        use crate::object::Stream;
        use crate::reader::{Reader, ReaderContext, ReaderExt};
        use crate::xref::DUMMY_XREF;

        // Build a minimal indirect stream object with /N 3 /First 6:
        // "1 0 obj <</N 3/First 6/Length 4>>stream\n1 0\nendstream\nendobj\n"
        let raw: &[u8] = b"1 0 obj <</N 3 /First 6 /Length 4>>\nstream\n1 0 \nendstream\nendobj\n";
        let mut r = Reader::new(raw);
        let ctx = ReaderContext::new(&DUMMY_XREF, false);
        let stream: Stream<'_> = r
            .read_with_context::<crate::object::indirect::IndirectObject<Stream<'_>>>(&ctx)
            .expect("synthetic stream should parse")
            .get();

        // The stream body has only "1 0 " — three entries cannot be
        // recovered, so the helper must return `None`.
        let body: &[u8] = b"1 0 ";
        assert!(
            parse_object_stream_offsets(&stream, body).is_none(),
            "truncated headers must not produce a partial offsets table"
        );
    }

    /// QF2-B perf harness. Not a correctness test — it prints microbench
    /// numbers and only runs when explicitly requested with
    /// `--ignored qf2b_bench`. The harness compares **direct re-parse**
    /// of /ObjStm offsets tables (what the pre-QF2-B `ObjectStream::new`
    /// did on every `xref.get` of an /ObjStm-stored object) versus the
    /// QF2-B cached lookup. This isolates the parse cost from the
    /// downstream object-decoding cost, which dominates `xref.get` and
    /// would otherwise hide the cache win.
    #[test]
    #[ignore = "perf measurement; run with `cargo test --release -- --ignored qf2b_bench`"]
    fn qf2b_bench_offsets_parse_vs_cached() {
        use std::time::Instant;

        // 161 /ObjStm headers, 575 KB. Walking and decoding 30+ ObjStms
        // here is enough to measure the parse delta cleanly.
        let path = "../../corpus/f3800.pdf";
        let Ok(bytes) = std::fs::read(path) else {
            eprintln!("[qf2b_bench] fixture {path} unavailable; skipping");
            return;
        };
        let pdf = Pdf::new(bytes).expect("load f3800.pdf");
        let xref = pdf.xref();

        // Warm cache via a single full sweep so we know which /ObjStms
        // exist.
        let max_id = (xref.len() as i32).min(3000);
        for n in 1..=max_id {
            let id = crate::object::ObjectIdentifier::new(n, 0);
            let _: Option<crate::object::Object<'_>> = xref.get(id);
        }
        let cached_objstms = xref.object_stream_offsets_cache_len();
        assert!(
            cached_objstms >= 5,
            "fixture must trigger several /ObjStms (got {cached_objstms})"
        );

        // Collect the object-stream container ids by looking them up in
        // the xref entries. We then iterate the cache to compare timing
        // for parse-from-scratch vs cache-hit.
        let mut objstm_ids: Vec<crate::object::ObjectIdentifier> = Vec::new();
        for n in 1..=max_id {
            let id = crate::object::ObjectIdentifier::new(n, 0);
            // Only ObjStm container ids resolve as Stream + /Type ObjStm.
            if let Some(stream) = xref.get::<crate::object::Stream<'_>>(id)
                && stream
                    .dict()
                    .get::<crate::object::Name>(crate::object::dict::keys::TYPE)
                    .as_deref()
                    == Some(b"ObjStm")
            {
                objstm_ids.push(id);
            }
            if objstm_ids.len() >= cached_objstms {
                break;
            }
        }
        let containers = objstm_ids.len();
        assert!(containers > 0);

        // Direct re-parse loop — mirrors pre-QF2-B behaviour: parse the
        // offsets table from scratch every time, no cache.
        const REPEATS: u32 = 200;
        let mut sink_parse = 0usize;
        let t_parse = Instant::now();
        for _ in 0..REPEATS {
            for id in &objstm_ids {
                let stream = xref
                    .get::<crate::object::Stream<'_>>(*id)
                    .expect("stream resolves");
                let Ok(decoded) = stream.decoded() else {
                    continue;
                };
                if let Some(offs) = parse_object_stream_offsets(&stream, &decoded) {
                    sink_parse = sink_parse.wrapping_add(offs.len());
                }
            }
        }
        let parse_elapsed = t_parse.elapsed();

        // Cache-hit loop — mirrors QF2-B behaviour: retrieve the same
        // parsed table from the per-document cache.
        let inner = match &xref.0 {
            crate::xref::Inner::Some(r) => r.clone(),
            _ => unreachable!(),
        };
        let mut sink_cache = 0usize;
        let t_cache = Instant::now();
        for _ in 0..REPEATS {
            for id in &objstm_ids {
                let offs = inner
                    .data
                    .get_object_stream_offsets_or_init(*id, || {
                        let stream = xref
                            .get::<crate::object::Stream<'_>>(*id)
                            .expect("stream resolves");
                        let decoded = stream.decoded().ok()?;
                        parse_object_stream_offsets(&stream, &decoded)
                    })
                    .expect("cached entry must exist after warm-up");
                sink_cache = sink_cache.wrapping_add(offs.len());
            }
        }
        let cache_elapsed = t_cache.elapsed();

        assert_eq!(
            sink_parse, sink_cache,
            "parsed and cached results must agree on offset-count totals"
        );

        let speedup = parse_elapsed.as_secs_f64() / cache_elapsed.as_secs_f64().max(1e-9);
        let reduction = (1.0 - cache_elapsed.as_secs_f64() / parse_elapsed.as_secs_f64()) * 100.0;

        eprintln!("[qf2b_bench] fixture: f3800.pdf");
        eprintln!("[qf2b_bench] /ObjStm containers measured: {containers}");
        eprintln!("[qf2b_bench] iterations per container:    {REPEATS}");
        eprintln!("[qf2b_bench] direct re-parse total:       {parse_elapsed:?}");
        eprintln!("[qf2b_bench] cached lookup total:         {cache_elapsed:?}");
        eprintln!("[qf2b_bench] speedup:                     {speedup:.1}x");
        eprintln!("[qf2b_bench] parse-time reduction:        {reduction:.1}%");

        // Acceptance gate: QF2-B target is ≥ 10 % reduction on parse path.
        // The microbench should show much more than that, since the cache
        // hit is O(1) hashmap fetch + Arc clone vs O(N) memchr+nom parse.
        assert!(
            reduction >= 10.0,
            "QF2-B acceptance: ≥ 10 % parse-time reduction required; got {reduction:.2} %"
        );
    }
}

/// Regression tests for fixes ported from hayro upstream (LaurenzV/hayro#1194).
///
/// Both subsection-header fields come from the file. `start + num_entries` and
/// `XREF_ENTRY_LEN * num_entries` overflowed on a header that declares more
/// entries than the address space holds.
#[cfg(test)]
mod upstream_hardening_tests {
    use super::*;

    #[test]
    fn xref_table_trailer_rejects_overflowing_entry_skip() {
        let data = b"xref\n0 999999999999999999999\ntrailer\n<<>>";
        let mut reader = Reader::new(data);
        assert!(read_xref_table_trailer(&mut reader, &ReaderContext::dummy()).is_none());
    }

    #[test]
    fn xref_table_trailer_still_reads_a_sane_header() {
        let data = b"xref\n0 1\n0000000000 65535 f \ntrailer\n<< /Size 1 >>";
        let mut reader = Reader::new(data);
        assert!(read_xref_table_trailer(&mut reader, &ReaderContext::dummy()).is_some());
    }
}
