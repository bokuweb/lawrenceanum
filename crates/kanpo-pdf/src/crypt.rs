//! 標準セキュリティハンドラの復号（ISO 32000-2 7.6）。
//!
//! デジタル官報の PDF は AES-256（V5 / R6）で暗号化されており、利用者パスワードは空。
//! 空パスワードで鍵を作って文字列とストリームを復号する。RC4・AES-128 の古い版も扱う。

use aes::cipher::{block_padding::NoPadding, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use md5::{Digest as _, Md5};
use sha2::{Sha256, Sha384, Sha512};

use crate::object::{Dict, ObjRef, Object};
use crate::{Error, Result};

const PAD: [u8; 32] = [
    0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41, 0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA, 0x01, 0x08, 0x2E, 0x2E, 0x00,
    0xB6, 0xD0, 0x68, 0x3E, 0x80, 0x2F, 0x0C, 0xA9, 0xFE, 0x64, 0x53, 0x69, 0x7A,
];

#[derive(Debug, Clone, Copy, PartialEq)]
enum Method {
    None,
    Rc4,
    AesV2,
    AesV3,
}

#[derive(Debug, Clone)]
pub(crate) struct Decryptor {
    key: Vec<u8>,
    stream: Method,
    string: Method,
}

fn get<'a>(d: &'a Dict, k: &[u8]) -> Option<&'a Object> {
    d.get(k)
}

impl Decryptor {
    /// `/Encrypt` 辞書と文書 ID の先頭要素から、空パスワードで鍵を作る。
    pub(crate) fn new(enc: &Dict, id0: &[u8]) -> Result<Self> {
        if get(enc, b"Filter").and_then(Object::as_name) != Some(b"Standard") {
            return Err(Error::Unsupported("non-standard security handler".into()));
        }
        let v = get(enc, b"V").and_then(Object::as_i64).unwrap_or(0);
        let r = get(enc, b"R").and_then(Object::as_i64).unwrap_or(2);
        let bytes = |k: &[u8]| {
            get(enc, k)
                .and_then(Object::as_str)
                .map(|s| s.to_vec())
                .unwrap_or_default()
        };
        let (o, u) = (bytes(b"O"), bytes(b"U"));

        let method_of = |name: Option<&[u8]>| -> Method {
            match name {
                None | Some(b"Identity") => Method::None,
                Some(n) => {
                    let cfm = get(enc, b"CF")
                        .and_then(Object::as_dict)
                        .and_then(|cf| cf.get(n))
                        .and_then(Object::as_dict)
                        .and_then(|f| f.get(&b"CFM"[..]))
                        .and_then(Object::as_name);
                    match cfm {
                        Some(b"AESV2") => Method::AesV2,
                        Some(b"AESV3") => Method::AesV3,
                        Some(b"None") => Method::None,
                        _ => Method::Rc4,
                    }
                }
            }
        };
        let (stream, string) = if v >= 4 {
            (
                method_of(get(enc, b"StmF").and_then(Object::as_name)),
                method_of(get(enc, b"StrF").and_then(Object::as_name)),
            )
        } else {
            (Method::Rc4, Method::Rc4)
        };

        let key = if r >= 5 {
            let ue = bytes(b"UE");
            if u.len() < 48 || ue.len() < 32 {
                return Err(Error::Malformed("short /U or /UE".into()));
            }
            let hash = |salt: &[u8]| {
                if r >= 6 {
                    hash_2b(b"", salt, &[])
                } else {
                    Sha256::digest(salt).to_vec()
                }
            };
            // 利用者パスワード（空）の検証用 salt で U の先頭 32 バイトと一致するか。
            if hash(&u[32..40]) != u[..32] {
                return Err(Error::Encrypted);
            }
            let ikey = hash(&u[40..48]);
            let mut buf = ue[..32].to_vec();
            cbc::Decryptor::<aes::Aes256>::new_from_slices(&ikey, &[0u8; 16])
                .map_err(|_| Error::Malformed("bad key".into()))?
                .decrypt_padded_mut::<NoPadding>(&mut buf)
                .map_err(|_| Error::Malformed("bad /UE".into()))?;
            buf
        } else {
            let len = if r == 2 {
                5
            } else {
                (get(enc, b"Length").and_then(Object::as_i64).unwrap_or(40) / 8).clamp(5, 16) as usize
            };
            let p = get(enc, b"P").and_then(Object::as_i64).unwrap_or(0) as i32;
            let mut h = Md5::new();
            h.update(PAD);
            h.update(&o);
            h.update(p.to_le_bytes());
            h.update(id0);
            let encrypt_metadata = !matches!(get(enc, b"EncryptMetadata"), Some(Object::Bool(false)));
            if r >= 4 && !encrypt_metadata {
                h.update([0xff; 4]);
            }
            let mut key = h.finalize().to_vec();
            if r >= 3 {
                for _ in 0..50 {
                    key = Md5::digest(&key[..len]).to_vec();
                }
            }
            key.truncate(len);
            key
        };
        Ok(Decryptor { key, stream, string })
    }

    pub(crate) fn decrypt_string(&self, r: ObjRef, data: &[u8]) -> Vec<u8> {
        self.apply(self.string, r, data)
    }

    pub(crate) fn decrypt_stream(&self, r: ObjRef, data: &[u8]) -> Vec<u8> {
        self.apply(self.stream, r, data)
    }

    fn apply(&self, method: Method, r: ObjRef, data: &[u8]) -> Vec<u8> {
        match method {
            Method::None => data.to_vec(),
            Method::AesV3 => aes_cbc_decrypt(&self.key, data),
            Method::Rc4 => rc4(&self.object_key(r, false), data),
            Method::AesV2 => aes_cbc_decrypt(&self.object_key(r, true), data),
        }
    }

    fn object_key(&self, r: ObjRef, aes: bool) -> Vec<u8> {
        let mut h = Md5::new();
        h.update(&self.key);
        h.update(&r.num.to_le_bytes()[..3]);
        h.update(r.gen.to_le_bytes());
        if aes {
            h.update(b"sAlT");
        }
        let mut k = h.finalize().to_vec();
        k.truncate((self.key.len() + 5).min(16));
        k
    }
}

/// ISO 32000-2 Algorithm 2.B（R6 のハッシュ）。利用者パスワードでは `udata` は空。
fn hash_2b(password: &[u8], salt: &[u8], udata: &[u8]) -> Vec<u8> {
    let mut k = Sha256::digest([password, salt, udata].concat()).to_vec();
    let mut round = 0usize;
    loop {
        let unit = [password, &k, udata].concat();
        let mut k1 = unit.repeat(64);
        let len = k1.len();
        let e = cbc::Encryptor::<aes::Aes128>::new_from_slices(&k[..16], &k[16..32])
            .expect("16-byte key and iv")
            .encrypt_padded_mut::<NoPadding>(&mut k1, len)
            .expect("multiple of block size")
            .to_vec();
        // 先頭 16 バイトを 128 ビットの整数とみた 3 の剰余は、各バイトの和の 3 の剰余に等しい。
        k = match e[..16].iter().map(|&b| b as u32).sum::<u32>() % 3 {
            0 => Sha256::digest(&e).to_vec(),
            1 => Sha384::digest(&e).to_vec(),
            _ => Sha512::digest(&e).to_vec(),
        };
        round += 1;
        if round >= 64 && (*e.last().unwrap() as usize) <= round - 32 {
            break;
        }
    }
    k.truncate(32);
    k
}

fn aes_cbc_decrypt(key: &[u8], data: &[u8]) -> Vec<u8> {
    if data.len() < 32 {
        return Vec::new();
    }
    let (iv, body) = data.split_at(16);
    let mut buf = body[..body.len() / 16 * 16].to_vec();
    let ok = match key.len() {
        16 => cbc::Decryptor::<aes::Aes128>::new_from_slices(key, iv)
            .ok()
            .and_then(|d| d.decrypt_padded_mut::<NoPadding>(&mut buf).ok().map(|_| ())),
        32 => cbc::Decryptor::<aes::Aes256>::new_from_slices(key, iv)
            .ok()
            .and_then(|d| d.decrypt_padded_mut::<NoPadding>(&mut buf).ok().map(|_| ())),
        _ => None,
    };
    if ok.is_none() {
        return Vec::new();
    }
    // PKCS#7 の詰め物を外す（壊れていたらそのまま）。
    if let Some(&n) = buf.last() {
        let n = n as usize;
        if (1..=16).contains(&n) && buf.len() >= n && buf[buf.len() - n..].iter().all(|&b| b as usize == n) {
            buf.truncate(buf.len() - n);
        }
    }
    buf
}

fn rc4(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut s: Vec<u8> = (0..=255).collect();
    let mut j = 0u8;
    for i in 0..256 {
        j = j.wrapping_add(s[i]).wrapping_add(key[i % key.len()]);
        s.swap(i, j as usize);
    }
    let (mut i, mut j) = (0u8, 0u8);
    data.iter()
        .map(|&b| {
            i = i.wrapping_add(1);
            j = j.wrapping_add(s[i as usize]);
            s.swap(i as usize, j as usize);
            b ^ s[s[i as usize].wrapping_add(s[j as usize]) as usize]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rc4_known_vector() {
        // RFC 6229 ではなく古典的な "Key" / "Plaintext" の例。
        assert_eq!(
            rc4(b"Key", b"Plaintext"),
            vec![0xBB, 0xF3, 0x16, 0xE8, 0xD9, 0x40, 0xAF, 0x0A, 0xD3]
        );
    }
}
