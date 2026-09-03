// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Deterministic AES-256 revision 5 *test data*. Fixed keys/IVs are never secure.
use aes::{
    cipher::{
        block_padding::{NoPadding, Pkcs7},
        BlockEncrypt, BlockEncryptMut, KeyInit, KeyIvInit,
    },
    Aes256,
};
use lopdf::{dictionary, Document, Object, StringFormat};
use sha2::{Digest, Sha256};
fn string(bytes: Vec<u8>) -> Object {
    Object::String(bytes, StringFormat::Hexadecimal)
}
fn hash(parts: &[&[u8]]) -> Vec<u8> {
    let mut hash = Sha256::new();
    for p in parts {
        hash.update(p);
    }
    hash.finalize().to_vec()
}
fn cbc(key: &[u8], bytes: &[u8]) -> Vec<u8> {
    cbc::Encryptor::<Aes256>::new_from_slices(key, &[0; 16])
        .unwrap()
        .encrypt_padded_vec_mut::<NoPadding>(bytes)
}
pub fn aes256(doc: &mut Document) {
    let key = [0x42; 32];
    let validation = [0x11; 8];
    let salt = [0x22; 8];
    let mut user = hash(&[&validation]);
    user.extend(validation);
    user.extend(salt);
    let ue = cbc(&hash(&[&salt]), &key);
    let mut owner = hash(&[b"owner", &validation, &user]);
    owner.extend(validation);
    owner.extend(salt);
    let oe = cbc(&hash(&[b"owner", &salt, &user]), &key);
    let mut perms = [0xff; 16];
    perms[..4].copy_from_slice(&(-4i32).to_le_bytes());
    perms[8..12].copy_from_slice(b"Tadb");
    Aes256::new_from_slice(&key)
        .unwrap()
        .encrypt_block((&mut perms).into());
    fn encrypt(obj: &mut Object, key: &[u8]) {
        let payload = match obj {
            Object::String(bytes, _) => Some(bytes),
            Object::Stream(s) => Some(&mut s.content),
            _ => None,
        };
        if let Some(bytes) = payload {
            let iv = [0x33; 16];
            let mut output = iv.to_vec();
            output.extend(
                cbc::Encryptor::<Aes256>::new_from_slices(key, &iv)
                    .unwrap()
                    .encrypt_padded_vec_mut::<Pkcs7>(bytes),
            );
            *bytes = output;
        }
        match obj {
            Object::Dictionary(d) => {
                for (_, v) in d.iter_mut() {
                    encrypt(v, key)
                }
            }
            Object::Array(a) => {
                for v in a {
                    encrypt(v, key)
                }
            }
            Object::Stream(s) => {
                s.dict.set("Length", s.content.len() as i64);
                for (_, v) in s.dict.iter_mut() {
                    encrypt(v, key)
                }
            }
            _ => {}
        }
    }
    for obj in doc.objects.values_mut() {
        encrypt(obj, &key);
    }
    let id=doc.add_object(dictionary! { "Filter"=>"Standard", "V"=>5, "R"=>5, "Length"=>256, "P"=>-4, "EncryptMetadata"=>true, "O"=>string(owner), "U"=>string(user), "OE"=>string(oe), "UE"=>string(ue), "Perms"=>string(perms.to_vec()), "CF"=>dictionary! {"StdCF"=>dictionary! {"CFM"=>"AESV3", "Length"=>32, "AuthEvent"=>"DocOpen"}}, "StmF"=>"StdCF", "StrF"=>"StdCF" });
    doc.trailer.set("Encrypt", id);
}
