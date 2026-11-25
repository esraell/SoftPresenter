//! The keys of a session: the legacy pairing a sender may ask for, and the video stream's
//! cipher. No key made or read here is ever written to the log.
//!
//! Follows UxPlay's `lib/pairing.c` (copyright (C) 2018 Juho Vähä-Herttua, (C) 2021-2023 F.
//! Duncanh), `lib/mirror_buffer.c` (copyright (C) 2019 dsafa22, modified by Florian Draschbacher
//! and F. Duncanh) and the key steps of `lib/raop_handlers.h`; all LGPL-2.1-or-later.

use aes::cipher::{KeyIvInit, StreamCipher};
use ring::rand::SecureRandom;
use ring::signature::KeyPair;
use sha2::{Digest, Sha512};

type Aes128Ctr = ctr::Ctr128BE<aes::Aes128>;

/// The first 16 bytes of SHA-512 over the parts.
fn sha512_16(parts: &[&[u8]]) -> [u8; 16] {
    let mut hash = Sha512::new();
    for part in parts {
        hash.update(part);
    }
    let mut out = [0_u8; 16];
    out.copy_from_slice(&hash.finalize()[..16]);
    out
}

/// The receiver's Ed25519 key, made afresh each time the program starts. (Nothing depends on
/// it lasting until devices are remembered, which needs the on-screen code.)
pub struct Identity {
    pair: ring::signature::Ed25519KeyPair,
}

impl Identity {
    pub fn new() -> Option<Identity> {
        let mut seed = [0_u8; 32];
        ring::rand::SystemRandom::new().fill(&mut seed).ok()?;
        Identity::from_seed(&seed)
    }

    pub fn from_seed(seed: &[u8; 32]) -> Option<Identity> {
        Some(Identity { pair: ring::signature::Ed25519KeyPair::from_seed_unchecked(seed).ok()? })
    }

    pub fn public(&self) -> [u8; 32] {
        let mut out = [0_u8; 32];
        out.copy_from_slice(self.pair.public_key().as_ref());
        out
    }
}

/// One connection's legacy pairing (`pair-verify`): an X25519 exchange whose two public keys
/// each side signs. Nothing checks whose keys they are, so it proves nothing about the sender;
/// it gives a shared secret that is mixed into the stream's key.
#[derive(Default)]
pub struct Pairing {
    handshake: Option<Handshake>,
}

struct Handshake {
    secret: [u8; 32],
    ours: [u8; 32],
    theirs: [u8; 32],
    their_identity: [u8; 32],
}

impl Pairing {
    /// The first `pair-verify` message: the sender's fresh X25519 key and its Ed25519 key. The
    /// answer is our fresh X25519 key and our signature of both keys, encrypted.
    pub fn start(&mut self, identity: &Identity, their_key: &[u8; 32], their_identity: &[u8; 32]) -> Option<[u8; 96]> {
        self.handshake = None;
        let random = ring::rand::SystemRandom::new();
        let private = ring::agreement::EphemeralPrivateKey::generate(&ring::agreement::X25519, &random).ok()?;
        let mut ours = [0_u8; 32];
        ours.copy_from_slice(private.compute_public_key().ok()?.as_ref());
        let peer = ring::agreement::UnparsedPublicKey::new(&ring::agreement::X25519, their_key);
        let secret = ring::agreement::agree_ephemeral(private, &peer, |secret| {
            let mut out = [0_u8; 32];
            out.copy_from_slice(secret);
            out
        })
        .ok()?;
        let mut message = [0_u8; 64];
        message[..32].copy_from_slice(&ours);
        message[32..].copy_from_slice(their_key);
        let mut signature = [0_u8; 64];
        signature.copy_from_slice(identity.pair.sign(&message).as_ref());
        verify_cipher(&secret).apply_keystream(&mut signature);
        let mut answer = [0_u8; 96];
        answer[..32].copy_from_slice(&ours);
        answer[32..].copy_from_slice(&signature);
        self.handshake = Some(Handshake { secret, ours, theirs: *their_key, their_identity: *their_identity });
        Some(answer)
    }

    /// The second `pair-verify` message: the sender's signature of both keys, encrypted with
    /// the same cipher carried on past our own signature.
    pub fn finish(&mut self, signature: &[u8; 64]) -> bool {
        let Some(handshake) = self.handshake.as_ref() else { return false };
        let mut cipher = verify_cipher(&handshake.secret);
        let mut skipped = [0_u8; 64];
        cipher.apply_keystream(&mut skipped);
        let mut signature = *signature;
        cipher.apply_keystream(&mut signature);
        let mut message = [0_u8; 64];
        message[..32].copy_from_slice(&handshake.theirs);
        message[32..].copy_from_slice(&handshake.ours);
        let key = ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, handshake.their_identity);
        key.verify(&message, &signature).is_ok()
    }

    /// The stream's key as the sender uses it: with pairing begun, the first 16 bytes of
    /// SHA-512 of the key and the shared secret; without, the key as it is.
    pub fn session_key(&self, key: [u8; 16]) -> [u8; 16] {
        match &self.handshake {
            Some(handshake) => sha512_16(&[&key, &handshake.secret]),
            None => key,
        }
    }
}

fn verify_cipher(secret: &[u8; 32]) -> Aes128Ctr {
    let key = sha512_16(&[b"Pair-Verify-AES-Key", secret]);
    let iv = sha512_16(&[b"Pair-Verify-AES-IV", secret]);
    Aes128Ctr::new(&key.into(), &iv.into())
}

/// The mirrored video's cipher: AES-128 in CTR mode, one keystream running across all the
/// encrypted packets of a stream.
pub struct VideoCipher {
    cipher: Aes128Ctr,
}

impl VideoCipher {
    /// From the session's key and the stream's `streamConnectionID`, written as an unsigned
    /// decimal number.
    pub fn new(session_key: &[u8; 16], connection_id: u64) -> VideoCipher {
        let key = sha512_16(&[format!("AirPlayStreamKey{connection_id}").as_bytes(), session_key]);
        let iv = sha512_16(&[format!("AirPlayStreamIV{connection_id}").as_bytes(), session_key]);
        VideoCipher { cipher: Aes128Ctr::new(&key.into(), &iv.into()) }
    }

    pub fn decrypt(&mut self, payload: &mut [u8]) {
        self.cipher.apply_keystream(payload);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len() / 2).map(|i| u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).unwrap()).collect()
    }

    /// The expected bytes were made outside this program, with Python's `hashlib` and the
    /// `openssl` command (`openssl enc -aes-128-ctr`), from the formulas in the protocol notes:
    /// key = SHA-512("AirPlayStreamKey" + id + session key)[..16], the same with "…IV…", and
    /// AES-128-CTR over the plain bytes.
    #[test]
    fn the_video_cipher_matches_openssl() {
        let session_key: [u8; 16] = hex("000102030405060708090a0b0c0d0e0f").try_into().unwrap();
        let plain: Vec<u8> = (0..45_u8).collect();
        let mut cipher = VideoCipher::new(&session_key, 12_345_678_901_234_567_890);
        // In three pieces that do not fall on block edges: the keystream runs across them.
        let mut bytes = plain.clone();
        let (first, rest) = bytes.split_at_mut(7);
        let (second, third) = rest.split_at_mut(22);
        cipher.decrypt(first);
        cipher.decrypt(second);
        cipher.decrypt(third);
        assert_eq!(bytes, hex(include_str!("testdata/video_cipher.hex").trim()));
    }

    /// From Python's `hashlib`: SHA-512(key + secret)[..16].
    #[test]
    fn a_paired_session_mixes_the_shared_secret_into_the_key() {
        let key: [u8; 16] = hex("000102030405060708090a0b0c0d0e0f").try_into().unwrap();
        assert_eq!(Pairing::default().session_key(key), key);
        let secret = [0x42_u8; 32];
        assert_eq!(sha512_16(&[&key, &secret]).to_vec(), hex(include_str!("testdata/paired_key.hex").trim()));
    }

    /// A sender written here, with its own keys, pairs with the receiver: both sides get the
    /// same secret, each checks the other's signature, and a wrong signature is refused.
    #[test]
    fn pair_verify_agrees_with_a_sender_and_refuses_a_wrong_signature() {
        let identity = Identity::from_seed(&[5; 32]).unwrap();
        let random = ring::rand::SystemRandom::new();
        let sender_identity = ring::signature::Ed25519KeyPair::from_seed_unchecked(&[6; 32]).unwrap();
        let sender_public: [u8; 32] = sender_identity.public_key().as_ref().try_into().unwrap();
        for honest in [true, false] {
            let private = ring::agreement::EphemeralPrivateKey::generate(&ring::agreement::X25519, &random).unwrap();
            let sender_key: [u8; 32] = private.compute_public_key().unwrap().as_ref().try_into().unwrap();
            let mut pairing = Pairing::default();
            let answer = pairing.start(&identity, &sender_key, &sender_public).unwrap();
            let receiver_key: [u8; 32] = answer[..32].try_into().unwrap();
            let secret = ring::agreement::agree_ephemeral(
                private,
                &ring::agreement::UnparsedPublicKey::new(&ring::agreement::X25519, receiver_key),
                |secret| <[u8; 32]>::try_from(secret).unwrap(),
            )
            .unwrap();
            // The sender's side, written out separately from the receiver's code.
            let mut key = [0_u8; 16];
            key.copy_from_slice(&Sha512::digest([b"Pair-Verify-AES-Key".as_slice(), &secret].concat())[..16]);
            let mut iv = [0_u8; 16];
            iv.copy_from_slice(&Sha512::digest([b"Pair-Verify-AES-IV".as_slice(), &secret].concat())[..16]);
            let mut cipher = Aes128Ctr::new(&key.into(), &iv.into());
            let mut theirs: [u8; 64] = answer[32..].try_into().unwrap();
            cipher.apply_keystream(&mut theirs);
            let signed = [receiver_key, sender_key].concat();
            ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, identity.public())
                .verify(&signed, &theirs)
                .unwrap();
            let mut ours: [u8; 64] =
                sender_identity.sign(&[sender_key, receiver_key].concat()).as_ref().try_into().unwrap();
            if !honest {
                ours[3] ^= 1;
            }
            cipher.apply_keystream(&mut ours);
            assert_eq!(pairing.finish(&ours), honest);
            // The key is mixed with the secret once the exchange has begun, as the sender does.
            assert_eq!(pairing.session_key([1; 16]), sha512_16(&[&[1; 16], &secret]));
        }
        assert!(!Pairing::default().finish(&[0; 64]));
    }
}
