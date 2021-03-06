extern crate hacl_star;

use std::mem;
use super::CryptoResolver;
use params::{DHChoice, HashChoice, CipherChoice};
use types::{Random, Dh, Hash, Cipher};
use self::hacl_star::curve25519::{self, SecretKey, PublicKey};
use self::hacl_star::sha2::{Sha256, Sha512};
use self::hacl_star::chacha20poly1305;

use byteorder::{ByteOrder, LittleEndian};
use utils::copy_memory;

#[derive(Default)]
pub struct HaclStarResolver;

impl CryptoResolver for HaclStarResolver {
    fn resolve_rng(&self) -> Option<Box<Random + Send>> {
        None
    }

    fn resolve_dh(&self, choice: &DHChoice) -> Option<Box<Dh + Send>> {
        if let DHChoice::Curve25519 = choice {
            Some(Box::new(Dh25519::default()))
        } else {
            None
        }
    }

    fn resolve_hash(&self, choice: &HashChoice) -> Option<Box<Hash + Send>> {
        match *choice {
            HashChoice::SHA256 => Some(Box::new(HashSHA256::default())),
            HashChoice::SHA512 => Some(Box::new(HashSHA512::default())),
            _                  => None,
        }
    }

    fn resolve_cipher(&self, choice: &CipherChoice) -> Option<Box<Cipher + Send>> {
        match *choice {
            CipherChoice::ChaChaPoly => Some(Box::new(CipherChaChaPoly::default())),
            _                        => None,
        }
    }
}

#[derive(Default)]
pub struct Dh25519 {
    privkey: SecretKey,
    pubkey:  PublicKey,
}

#[derive(Default)]
pub struct CipherChaChaPoly {
    key: [u8; chacha20poly1305::KEY_LENGTH],
}

#[derive(Default)]
pub struct HashSHA256 {
    hasher: Sha256
}

#[derive(Default)]
pub struct HashSHA512 {
    hasher: Sha512
}

impl Dh for Dh25519 {
    fn name(&self) -> &'static str {
        static NAME: &'static str = "25519";
        NAME
    }

    fn pub_len(&self) -> usize {
        32
    }

    fn priv_len(&self) -> usize {
        32
    }

    fn set(&mut self, privkey: &[u8]) {
        copy_memory(privkey, &mut self.privkey.0); /* RUSTSUCKS: Why can't I convert slice -> array? */
        self.pubkey = self.privkey.get_public();
    }

    fn generate(&mut self, rng: &mut Random) {
        rng.fill_bytes(&mut self.privkey.0);
        self.pubkey = self.privkey.get_public();
    }

    fn pubkey(&self) -> &[u8] {
        &self.pubkey.0
    }

    fn privkey(&self) -> &[u8] {
        &self.privkey.0
    }

    fn dh(&self, pubkey: &[u8], out: &mut [u8]) -> Result<(), ()> {
        let out = array_mut_ref!(out, 0, 32);
        let pubkey = array_ref!(pubkey, 0, 32);
        curve25519::scalarmult(out, &self.privkey.0, pubkey);
        Ok(())
    }

}

impl Cipher for CipherChaChaPoly {
    fn name(&self) -> &'static str {
        "ChaChaPoly"
    }

    fn set(&mut self, key: &[u8]) {
        copy_memory(key, &mut self.key);
    }

    fn encrypt(&self, nonce: u64, authtext: &[u8], plaintext: &[u8], out: &mut [u8]) -> usize {
        let mut nonce_bytes = [0u8; 12];
        LittleEndian::write_u64(&mut nonce_bytes[4..], nonce);

        let (out, tag) = out.split_at_mut(plaintext.len());
        let tag = array_mut_ref!(tag, 0, chacha20poly1305::MAC_LENGTH);
        copy_memory(plaintext, out);

        chacha20poly1305::Key(&self.key)
            .nonce(&nonce_bytes)
            .encrypt(authtext, out, tag);

        out.len() + tag.len()
    }

    fn decrypt(&self, nonce: u64, authtext: &[u8], ciphertext: &[u8], out: &mut [u8]) -> Result<usize, ()> {
        let mut nonce_bytes = [0u8; 12];
        LittleEndian::write_u64(&mut nonce_bytes[4..], nonce);

        let len = ciphertext.len();
        let (ciphertext, tag) = ciphertext.split_at(len - chacha20poly1305::MAC_LENGTH);
        let tag = array_ref!(tag, 0, chacha20poly1305::MAC_LENGTH);
        let len = ciphertext.len();
        copy_memory(ciphertext, out);

        if chacha20poly1305::Key(&self.key)
            .nonce(&nonce_bytes)
            .decrypt(authtext, &mut out[..len], tag)
        {
            Ok(out.len())
        } else {
            Err(())
        }
    }
}

impl Hash for HashSHA256 {
    fn block_len(&self) -> usize {
        Sha256::BLOCK_LENGTH
    }

    fn hash_len(&self) -> usize {
        Sha256::HASH_LENGTH
    }

    fn name(&self) -> &'static str {
        "SHA256"
    }

    fn reset(&mut self) {
        self.hasher = Sha256::default();
    }

    fn input(&mut self, data: &[u8]) {
        self.hasher.update(data);
    }

    fn result(&mut self, out: &mut [u8]) {
        let out = array_mut_ref!(out, 0, 32);
        mem::replace(&mut self.hasher, Default::default()).finish(out);
    }
}

impl Hash for HashSHA512 {
    fn name(&self) -> &'static str {
        "SHA512"
    }

    fn block_len(&self) -> usize {
        Sha512::BLOCK_LENGTH
    }

    fn hash_len(&self) -> usize {
        Sha512::HASH_LENGTH
    }

    fn reset(&mut self) {
        self.hasher = Sha512::default();
    }

    fn input(&mut self, data: &[u8]) {
        self.hasher.update(data);
    }

    fn result(&mut self, out: &mut [u8]) {
        let out = array_mut_ref!(out, 0, 64);
        mem::replace(&mut self.hasher, Default::default()).finish(out);
    }
}


#[cfg(test)]
mod tests {

    extern crate hex;

    use types::*;
    use super::*;
    use self::hex::{FromHex, ToHex};
    use super::hacl_star::poly1305::Poly1305;

    #[test]
    fn test_sha256() {
        let mut output = [0u8; 32];
        let mut hasher:HashSHA256 = Default::default();
        hasher.input("abc".as_bytes());
        hasher.result(&mut output);
        assert!(hex::encode(output) == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn test_hmac_sha256_sha512() {
        let key = Vec::<u8>::from_hex("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();
        let data = Vec::<u8>::from_hex("dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd").unwrap();
        let mut output1 = [0u8; 32];
        let mut hasher: HashSHA256 = Default::default();
        hasher.hmac(&key, &data, &mut output1);
        assert!(hex::encode(output1) == "773ea91e36800e46854db8ebd09181a72959098b3ef8c122d9635514ced565fe");

        let mut output2 = [0u8; 64];
        let mut hasher: HashSHA512 = Default::default();
        hasher.hmac(&key, &data, &mut output2);
        assert!(hex::encode(output2.to_vec()) == "fa73b0089d56a284efb0f0756c890be9\
                                     b1b5dbdd8ee81a3655f83e33b2279d39\
                                     bf3e848279a722c806b485a47e67c807\
                                     b946a337bee8942674278859e13292fb");
    }

    #[test]
    fn test_curve25519() {
    // Curve25519 test - draft-curves-10
        let mut keypair:Dh25519 = Default::default();
        let scalar = Vec::<u8>::from_hex("a546e36bf0527c9d3b16154b82465edd62144c0ac1fc5a18506a2244ba449ac4").unwrap();
        copy_memory(&scalar, &mut keypair.privkey.0);
        let public = Vec::<u8>::from_hex("e6db6867583030db3594c1a424b15f7c726624ec26b3353b10a903a6d0ab1c4c").unwrap();
        let mut output = [0u8; 32];
        keypair.dh(&public, &mut output);
        assert!(hex::encode(output) == "c3da55379de9c6908e94ea4df28d084f32eccf03491c71f754b4075577a28552");
    }

    #[test]
    fn test_poly1305() {
    // Poly1305 internal test - RFC 7539
        let key = Vec::<u8>::from_hex("85d6be7857556d337f4452fe42d506a80103808afb0db2fd4abff6af4149f51b").unwrap();
        let msg = Vec::<u8>::from_hex("43727970746f6772617068696320466f\
                   72756d2052657365617263682047726f\
                   7570").unwrap();
        let key = array_ref!(key, 0, 32);
        let mut poly = Poly1305::new(key);
        poly.update(&msg);
        let mut output = [0u8; 16];
        poly.finish(&mut output);
        assert!(hex::encode(output) == "a8061dc1305136c6c22b8baf0c0127a9");
    }

    #[test]
    fn test_chachapoly_empty() {
    //ChaChaPoly round-trip test, empty plaintext
        let key = [0u8; 32];
        let nonce = 0u64;
        let plaintext = [0u8; 0];
        let authtext = [0u8; 0];
        let mut ciphertext = [0u8; 16];
        let mut cipher1 : CipherChaChaPoly = Default::default();
        cipher1.set(&key);
        cipher1.encrypt(nonce, &authtext, &plaintext, &mut ciphertext);

        let mut resulttext = [0u8; 1];
        let mut cipher2 : CipherChaChaPoly = Default::default();
        cipher2.set(&key);
        cipher2.decrypt(nonce, &authtext, &ciphertext, &mut resulttext).unwrap();
        assert!(resulttext[0] == 0);
        ciphertext[0] ^= 1;
        assert!(cipher2.decrypt(nonce, &authtext, &ciphertext, &mut resulttext).is_err());
    }

    #[test]
    fn test_chachapoly_nonempty() {
    //ChaChaPoly round-trip test, non-empty plaintext
        let key = [0u8; 32];
        let nonce = 0u64;
        let plaintext = [0x34u8; 117];
        let authtext = [0u8; 0];
        let mut ciphertext = [0u8; 133];
        let mut cipher1 : CipherChaChaPoly = Default::default();
        cipher1.set(&key);
        cipher1.encrypt(nonce, &authtext, &plaintext, &mut ciphertext);

        let mut resulttext = [0u8; 117];
        let mut cipher2 : CipherChaChaPoly = Default::default();
        cipher2.set(&key);
        cipher2.decrypt(nonce, &authtext, &ciphertext, &mut resulttext).unwrap();
        assert!(hex::encode(resulttext.to_vec()) == hex::encode(plaintext.to_vec()));
    }

    #[test]
    fn test_chachapoly_known_answer() {
    //ChaChaPoly known-answer test - RFC 7539
        let key =Vec::<u8>::from_hex("1c9240a5eb55d38af333888604f6b5f0\
                  473917c1402b80099dca5cbc207075c0").unwrap();
        let nonce = 0x0807060504030201u64;
        let ciphertext =Vec::<u8>::from_hex("64a0861575861af460f062c79be643bd\
                         5e805cfd345cf389f108670ac76c8cb2\
                         4c6cfc18755d43eea09ee94e382d26b0\
                         bdb7b73c321b0100d4f03b7f355894cf\
                         332f830e710b97ce98c8a84abd0b9481\
                         14ad176e008d33bd60f982b1ff37c855\
                         9797a06ef4f0ef61c186324e2b350638\
                         3606907b6a7c02b0f9f6157b53c867e4\
                         b9166c767b804d46a59b5216cde7a4e9\
                         9040c5a40433225ee282a1b0a06c523e\
                         af4534d7f83fa1155b0047718cbc546a\
                         0d072b04b3564eea1b422273f548271a\
                         0bb2316053fa76991955ebd63159434e\
                         cebb4e466dae5a1073a6727627097a10\
                         49e617d91d361094fa68f0ff77987130\
                         305beaba2eda04df997b714d6c6f2c29\
                         a6ad5cb4022b02709b").unwrap();
        let tag = Vec::<u8>::from_hex("eead9d67890cbb22392336fea1851f38").unwrap();
        let authtext = Vec::<u8>::from_hex("f33388860000000000004e91").unwrap();
        let mut combined_text = [0u8; 1024];
        let mut out = [0u8; 1024];
        copy_memory(&ciphertext, &mut combined_text);
        copy_memory(&tag[0..chacha20poly1305::MAC_LENGTH], &mut combined_text[ciphertext.len()..]);

        let mut cipher : CipherChaChaPoly = Default::default();
        cipher.set(&key);
        cipher.decrypt(nonce, &authtext, &combined_text[..ciphertext.len()+chacha20poly1305::MAC_LENGTH], &mut out[..ciphertext.len()]).unwrap();
        let desired_plaintext = "496e7465726e65742d44726166747320\
                                 61726520647261667420646f63756d65\
                                 6e74732076616c696420666f72206120\
                                 6d6178696d756d206f6620736978206d\
                                 6f6e74687320616e64206d6179206265\
                                 20757064617465642c207265706c6163\
                                 65642c206f72206f62736f6c65746564\
                                 206279206f7468657220646f63756d65\
                                 6e747320617420616e792074696d652e\
                                 20497420697320696e617070726f7072\
                                 6961746520746f2075736520496e7465\
                                 726e65742d4472616674732061732072\
                                 65666572656e6365206d617465726961\
                                 6c206f7220746f206369746520746865\
                                 6d206f74686572207468616e20617320\
                                 2fe2809c776f726b20696e2070726f67\
                                 726573732e2fe2809d";
        assert!(hex::encode(out[..ciphertext.len()].to_owned()) == desired_plaintext);
    }
}
