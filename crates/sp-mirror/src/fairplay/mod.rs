//! The FairPlay step of Screen Mirroring (AirPlay): answering a sender's two `fp-setup`
//! messages, and turning the 72-byte key it then sends into the 16-byte AES key of the stream.
//!
//! This module is a Rust port of **playfair** (github.com/EstebanKubata/playfair, 2015), as
//! carried in UxPlay's `lib/playfair/` at commit fbd040e: `playfair.c`, `omg_hax.c`,
//! `modified_md5.c`, `sap_hash.c` and `hand_garble.c`, function by function, with the tables of
//! `omg_hax.h`. playfair is under the GNU General Public License, version 3 (the text is in the
//! repository's `LICENSE`); so is this module. Only what `playfair_decrypt` needs is ported.
//! The two `fp-setup` answers follow UxPlay's `lib/fairplay_playfair.c` (copyright (C) 2018 Juho
//! Vähä-Herttua, (C) 2026 Omar Roth; LGPL-2.1-or-later).
//!
//! The port is checked against answers computed once by the original C (`vectors.txt`).
//! `docs/licences/screen-mirroring.md` has the notices in full.

// The loops keep the original's shape (an index into several arrays at once), so that the
// port can be read beside the C.
#![allow(clippy::needless_range_loop)]

mod garble;
mod replies;
mod tables;

use tables::{
    DEFAULT_SAP, INDEX_MANGLE, INITIAL_SESSION_KEY, MESSAGE_IV, MESSAGE_KEY, STATIC_SOURCE_1, STATIC_SOURCE_2, T_KEY,
    TABLE_S1, TABLE_S2, TABLE_S3, TABLE_S4, TABLE_S5, TABLE_S6, TABLE_S7, TABLE_S8, TABLE_S9, TABLE_S10, X_KEY, Z_KEY,
};

/// The length of a sender's first `fp-setup` message.
pub const SETUP_LEN: usize = 16;
/// The length of the answer to it.
pub const SETUP_REPLY_LEN: usize = 142;
/// The length of a sender's second `fp-setup` message.
pub const HANDSHAKE_LEN: usize = 164;
/// The length of the answer to it.
pub const HANDSHAKE_REPLY_LEN: usize = 32;
/// The length of the scrambled key a sender puts in `SETUP`.
pub const EKEY_LEN: usize = 72;

/// What one connection knows of the FairPlay step: the second message, once it has come.
#[derive(Default)]
pub struct FairPlay {
    message: Option<[u8; HANDSHAKE_LEN]>,
}

/// Why a FairPlay message was refused.
#[derive(Debug, PartialEq, Eq)]
pub enum Refused {
    /// Not the version of FairPlay that screen mirroring uses (3), or a mode outside 0 to 3.
    Unsupported,
    /// The key came before the second message.
    NoHandshake,
}

impl FairPlay {
    /// The answer to the first message: one of four fixed messages, chosen by the mode.
    pub fn setup(&mut self, request: &[u8; SETUP_LEN]) -> Result<[u8; SETUP_REPLY_LEN], Refused> {
        self.message = None;
        if request[4] != 0x03 || request[14] >= 4 {
            return Err(Refused::Unsupported);
        }
        let mode = usize::from(request[14]);
        let mut reply = [0_u8; SETUP_REPLY_LEN];
        reply.copy_from_slice(&replies::REPLY_MESSAGE[mode * SETUP_REPLY_LEN..(mode + 1) * SETUP_REPLY_LEN]);
        Ok(reply)
    }

    /// The answer to the second message: a fixed header and the message's last 20 bytes. The
    /// message is kept: the key is unscrambled with it.
    pub fn handshake(&mut self, request: &[u8; HANDSHAKE_LEN]) -> Result<[u8; HANDSHAKE_REPLY_LEN], Refused> {
        self.message = None;
        if request[4] != 0x03 || request[12] >= 4 {
            return Err(Refused::Unsupported);
        }
        self.message = Some(*request);
        let mut reply = [0_u8; HANDSHAKE_REPLY_LEN];
        reply[..12].copy_from_slice(&replies::FP_HEADER);
        reply[12..].copy_from_slice(&request[144..164]);
        Ok(reply)
    }

    /// The stream's AES key, from the scrambled key in `SETUP`.
    pub fn decrypt(&self, ekey: &[u8; EKEY_LEN]) -> Result<[u8; 16], Refused> {
        let message = self.message.as_ref().ok_or(Refused::NoHandshake)?;
        Ok(playfair_decrypt(message, ekey))
    }
}

// ---- playfair.c -------------------------------------------------------------------------------

fn playfair_decrypt(message3: &[u8; HANDSHAKE_LEN], cipher_text: &[u8; EKEY_LEN]) -> [u8; 16] {
    let chunk1 = &cipher_text[16..32];
    let chunk2 = &cipher_text[56..72];
    let sap_key = generate_session_key(&DEFAULT_SAP, message3);
    let key_schedule = generate_key_schedule(&sap_key);
    let mut block_in = [0_u8; 16];
    for i in 0..16 {
        block_in[i] = chunk2[i] ^ Z_KEY[i];
    }
    cycle(&mut block_in, &key_schedule);
    let mut key_out = [0_u8; 16];
    for i in 0..16 {
        key_out[i] = block_in[i] ^ chunk1[i];
    }
    for i in 0..16 {
        key_out[i] ^= X_KEY[i];
    }
    for i in 0..16 {
        key_out[i] ^= Z_KEY[i];
    }
    key_out
}

// ---- omg_hax.c --------------------------------------------------------------------------------

/// The 32-bit word at a place in a block, as the original reads it through a pointer cast (on
/// the little-endian computers it was written for).
fn word(block: &[u8], index: usize) -> u32 {
    u32::from_le_bytes([block[index * 4], block[index * 4 + 1], block[index * 4 + 2], block[index * 4 + 3]])
}

fn set_word(block: &mut [u8], index: usize, value: u32) {
    block[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
}

fn xor_word(block: &mut [u8], index: usize, value: u32) {
    let value = word(block, index) ^ value;
    set_word(block, index, value);
}

fn table_index(i: usize) -> &'static [u8] {
    let at = ((31 * i) % 0x28) << 8;
    &TABLE_S1[at..at + 256]
}

fn message_table_index(i: usize) -> &'static [u8] {
    let at = ((97 * i) % 144) << 8;
    &TABLE_S2[at..at + 256]
}

fn permute_table_2(i: usize) -> &'static [u8] {
    let at = ((71 * i) % 144) << 8;
    &TABLE_S4[at..at + 256]
}

fn permute_block_1(block: &mut [u8; 16]) {
    let s3 = |at: usize, byte: u8| TABLE_S3[at + usize::from(byte)];
    block[0] = s3(0x000, block[0]);
    block[4] = s3(0x400, block[4]);
    block[8] = s3(0x800, block[8]);
    block[12] = s3(0xc00, block[12]);
    let tmp = block[13];
    block[13] = s3(0x100, block[9]);
    block[9] = s3(0xd00, block[5]);
    block[5] = s3(0x900, block[1]);
    block[1] = s3(0x500, tmp);
    let tmp = block[2];
    block[2] = s3(0xa00, block[10]);
    block[10] = s3(0x200, tmp);
    let tmp = block[6];
    block[6] = s3(0xe00, block[14]);
    block[14] = s3(0x600, tmp);
    let tmp = block[3];
    block[3] = s3(0xf00, block[7]);
    block[7] = s3(0x300, block[11]);
    block[11] = s3(0x700, block[15]);
    block[15] = s3(0xb00, tmp);
}

fn permute_block_2(block: &mut [u8; 16], round: usize) {
    let p = |at: usize, byte: u8| permute_table_2(round * 16 + at)[usize::from(byte)];
    block[0] = p(0, block[0]);
    block[4] = p(4, block[4]);
    block[8] = p(8, block[8]);
    block[12] = p(12, block[12]);
    let tmp = block[13];
    block[13] = p(13, block[9]);
    block[9] = p(9, block[5]);
    block[5] = p(5, block[1]);
    block[1] = p(1, tmp);
    let tmp = block[2];
    block[2] = p(2, block[10]);
    block[10] = p(10, tmp);
    let tmp = block[6];
    block[6] = p(6, block[14]);
    block[14] = p(14, tmp);
    let tmp = block[3];
    block[3] = p(3, block[7]);
    block[7] = p(7, block[11]);
    block[11] = p(11, block[15]);
    block[15] = p(15, tmp);
}

fn generate_key_schedule(key_material: &[u8; 16]) -> [[u32; 4]; 11] {
    let mut key_schedule = [[0_u32; 4]; 11];
    let mut buffer = [0_u8; 16];
    for i in 0..16 {
        buffer[i] = key_material[i] ^ T_KEY[i];
    }
    let mut ti = 0;
    for round in 0..11 {
        key_schedule[round][0] = word(&buffer, 0);
        let table1 = table_index(ti);
        let table2 = table_index(ti + 1);
        let table3 = table_index(ti + 2);
        let table4 = table_index(ti + 3);
        ti += 4;
        buffer[0] ^= table1[usize::from(buffer[0x0d])] ^ INDEX_MANGLE[round];
        buffer[1] ^= table2[usize::from(buffer[0x0e])];
        buffer[2] ^= table3[usize::from(buffer[0x0f])];
        buffer[3] ^= table4[usize::from(buffer[0x0c])];
        key_schedule[round][1] = word(&buffer, 1);
        let lower = word(&buffer, 0);
        xor_word(&mut buffer, 1, lower);
        key_schedule[round][2] = word(&buffer, 2);
        let lower = word(&buffer, 1);
        xor_word(&mut buffer, 2, lower);
        key_schedule[round][3] = word(&buffer, 3);
        let lower = word(&buffer, 2);
        xor_word(&mut buffer, 3, lower);
    }
    key_schedule
}

fn cycle(block: &mut [u8; 16], key_schedule: &[[u32; 4]; 11]) {
    for i in 0..4 {
        xor_word(block, i, key_schedule[10][i]);
    }
    permute_block_1(block);
    for round in 0..9 {
        let keys = &key_schedule[9 - round];
        for i in 0..4 {
            let key = keys[i].to_le_bytes();
            let ab = TABLE_S5[usize::from(block[i * 4 + 3] ^ key[3])]
                ^ TABLE_S6[usize::from(block[i * 4 + 2] ^ key[2])]
                ^ TABLE_S7[usize::from(block[i * 4 + 1] ^ key[1])]
                ^ TABLE_S8[usize::from(block[i * 4] ^ key[0])];
            set_word(block, i, ab);
        }
        permute_block_2(block, 8 - round);
    }
    for i in 0..4 {
        xor_word(block, i, key_schedule[0][i]);
    }
}

fn decrypt_message(message_in: &[u8; HANDSHAKE_LEN]) -> [u8; 128] {
    let mut decrypted = [0_u8; 128];
    let mode = usize::from(message_in[12]);
    let message_key = &MESSAGE_KEY[mode * 144..(mode + 1) * 144];
    let message_iv = &MESSAGE_IV[mode * 16..(mode + 1) * 16];
    for i in 0..8 {
        let mut buffer = [0_u8; 16];
        for j in 0..16 {
            buffer[j] = if mode == 3 { message_in[(0x80 - 0x10 * i) + j] } else { message_in[(0x10 * (i + 1)) + j] };
        }
        for j in 0..9 {
            let base = 0x80 - 0x10 * j;
            let m = |at: usize, byte: u8| message_table_index(base + at)[usize::from(byte)] ^ message_key[base + at];
            buffer[0x0] = m(0x0, buffer[0x0]);
            buffer[0x4] = m(0x4, buffer[0x4]);
            buffer[0x8] = m(0x8, buffer[0x8]);
            buffer[0xc] = m(0xc, buffer[0xc]);
            let tmp = buffer[0x0d];
            buffer[0xd] = m(0xd, buffer[0x9]);
            buffer[0x9] = m(0x9, buffer[0x5]);
            buffer[0x5] = m(0x5, buffer[0x1]);
            buffer[0x1] = m(0x1, tmp);
            let tmp = buffer[0x02];
            buffer[0x2] = m(0x2, buffer[0xa]);
            buffer[0xa] = m(0xa, tmp);
            let tmp = buffer[0x06];
            buffer[0x6] = m(0x6, buffer[0xe]);
            buffer[0xe] = m(0xe, tmp);
            let tmp = buffer[0x3];
            buffer[0x3] = m(0x3, buffer[0x7]);
            buffer[0x7] = m(0x7, buffer[0xb]);
            buffer[0xb] = m(0xb, buffer[0xf]);
            buffer[0xf] = m(0xf, tmp);
            for k in 0..4 {
                let mixed = TABLE_S9[usize::from(buffer[k * 4])]
                    ^ TABLE_S9[0x100 + usize::from(buffer[k * 4 + 1])]
                    ^ TABLE_S9[0x200 + usize::from(buffer[k * 4 + 2])]
                    ^ TABLE_S9[0x300 + usize::from(buffer[k * 4 + 3])];
                set_word(&mut buffer, k, mixed);
            }
        }
        let s10 = |at: usize, byte: u8| TABLE_S10[(at << 8) + usize::from(byte)];
        buffer[0x0] = s10(0x0, buffer[0x0]);
        buffer[0x4] = s10(0x4, buffer[0x4]);
        buffer[0x8] = s10(0x8, buffer[0x8]);
        buffer[0xc] = s10(0xc, buffer[0xc]);
        let tmp = buffer[0x0d];
        buffer[0xd] = s10(0xd, buffer[0x9]);
        buffer[0x9] = s10(0x9, buffer[0x5]);
        buffer[0x5] = s10(0x5, buffer[0x1]);
        buffer[0x1] = s10(0x1, tmp);
        let tmp = buffer[0x02];
        buffer[0x2] = s10(0x2, buffer[0xa]);
        buffer[0xa] = s10(0xa, tmp);
        let tmp = buffer[0x06];
        buffer[0x6] = s10(0x6, buffer[0xe]);
        buffer[0xe] = s10(0xe, tmp);
        let tmp = buffer[0x3];
        buffer[0x3] = s10(0x3, buffer[0x7]);
        buffer[0x7] = s10(0x7, buffer[0xb]);
        buffer[0xb] = s10(0xb, buffer[0xf]);
        buffer[0xf] = s10(0xf, tmp);
        // The first 0x10 bytes of the message are its header.
        let (other, out): (&[u8], usize) = if mode == 3 {
            if i < 7 { (&message_in[0x70 - 0x10 * i..0x80 - 0x10 * i], 0x70 - 0x10 * i) } else { (message_iv, 0) }
        } else if i > 0 {
            (&message_in[0x10 * i..0x10 * (i + 1)], 0x10 * i)
        } else {
            (message_iv, 0)
        };
        for j in 0..16 {
            decrypted[out + j] = buffer[j] ^ other[j];
        }
    }
    decrypted
}

fn generate_session_key(old_sap: &[u8], message_in: &[u8; HANDSHAKE_LEN]) -> [u8; 16] {
    let decrypted = decrypt_message(message_in);
    let mut new_sap = [0_u8; 320];
    new_sap[..0x11].copy_from_slice(&STATIC_SOURCE_1);
    new_sap[0x11..0x91].copy_from_slice(&decrypted);
    new_sap[0x91..0x111].copy_from_slice(&old_sap[0x80..0x100]);
    new_sap[0x111..0x140].copy_from_slice(&STATIC_SOURCE_2);
    let mut session_key = INITIAL_SESSION_KEY;
    for round in 0..5 {
        let mut base = [0_u8; 64];
        base.copy_from_slice(&new_sap[round * 64..(round + 1) * 64]);
        let md5 = modified_md5(&base, &session_key);
        sap_hash(&base, &mut session_key);
        for i in 0..4 {
            let sum = word(&session_key, i).wrapping_add(word(&md5, i));
            set_word(&mut session_key, i, sum);
        }
    }
    for i in (0..16).step_by(4) {
        session_key.swap(i, i + 3);
        session_key.swap(i + 1, i + 2);
    }
    for byte in &mut session_key {
        *byte ^= 121;
    }
    session_key
}

// ---- modified_md5.c ---------------------------------------------------------------------------

const SHIFT: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20,
    4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15,
    21,
];

/// The original computes each round's constant as the whole part of 2^32 × |sin(i + 1)|; these
/// are those numbers (MD5's own), written out so that no computer's `sin` can differ.
const SINES: [u32; 64] = [
    0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501, 0x698098d8,
    0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340,
    0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8, 0x21e1cde6, 0xc33707d6, 0xf4d50d87,
    0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c,
    0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039,
    0xe6db99e5, 0x1fa27cf8, 0xc4ac5665, 0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92,
    0xffeff47d, 0x85845dd1, 0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb,
    0xeb86d391,
];

fn modified_md5(original_block_in: &[u8; 64], key_in: &[u8; 16]) -> [u8; 16] {
    let mut block_in = *original_block_in;
    let mut a = word(key_in, 0);
    let mut b = word(key_in, 1);
    let mut c = word(key_in, 2);
    let mut d = word(key_in, 3);
    for i in 0..64_usize {
        let j = match i {
            0..16 => i,
            16..32 => (5 * i + 1) % 16,
            32..48 => (3 * i + 5) % 16,
            _ => 7 * i % 16,
        };
        let input =
            u32::from_be_bytes([block_in[4 * j], block_in[4 * j + 1], block_in[4 * j + 2], block_in[4 * j + 3]]);
        let mut z = a.wrapping_add(input).wrapping_add(SINES[i]);
        let mixed = match i {
            0..16 => (b & c) | (!b & d),
            16..32 => (b & d) | (c & !d),
            32..48 => b ^ c ^ d,
            _ => c ^ (b | !d),
        };
        z = z.wrapping_add(mixed).rotate_left(SHIFT[i]);
        z = z.wrapping_add(b);
        let tmp = d;
        d = c;
        c = b;
        b = z;
        a = tmp;
        if i == 31 {
            let mut swap = |x: u32, y: u32| {
                let (x, y) = (x as usize, y as usize);
                let (wx, wy) = (word(&block_in, x), word(&block_in, y));
                set_word(&mut block_in, x, wy);
                set_word(&mut block_in, y, wx);
            };
            swap(a & 15, b & 15);
            swap(c & 15, d & 15);
            swap((a & (15 << 4)) >> 4, (b & (15 << 4)) >> 4);
            swap((a & (15 << 8)) >> 8, (b & (15 << 8)) >> 8);
            swap((a & (15 << 12)) >> 12, (b & (15 << 12)) >> 12);
        }
    }
    let mut key_out = [0_u8; 16];
    set_word(&mut key_out, 0, word(key_in, 0).wrapping_add(a));
    set_word(&mut key_out, 1, word(key_in, 1).wrapping_add(b));
    set_word(&mut key_out, 2, word(key_in, 2).wrapping_add(c));
    set_word(&mut key_out, 3, word(key_in, 3).wrapping_add(d));
    key_out
}

// ---- sap_hash.c -------------------------------------------------------------------------------

fn sap_hash(block_in: &[u8; 64], key_out: &mut [u8; 16]) {
    use garble::rol8;
    let mut buffer0: [u8; 20] = [
        0x96, 0x5F, 0xC6, 0x53, 0xF8, 0x46, 0xCC, 0x18, 0xDF, 0xBE, 0xB2, 0xF8, 0x38, 0xD7, 0xEC, 0x22, 0x03, 0xD1,
        0x20, 0x8F,
    ];
    let mut buffer1 = [0_u8; 210];
    let mut buffer2: [u8; 35] = [
        0x43, 0x54, 0x62, 0x7A, 0x18, 0xC3, 0xD6, 0xB3, 0x9A, 0x56, 0xF6, 0x1C, 0x14, 0x3F, 0x0C, 0x1D, 0x3B, 0x36,
        0x83, 0xB1, 0x39, 0x51, 0x4A, 0xAA, 0x09, 0x3E, 0xFE, 0x44, 0xAF, 0xDE, 0xC3, 0x20, 0x9D, 0x42, 0x3A,
    ];
    let mut buffer3 = [0_u8; 132];
    let buffer4: [u8; 21] = [
        0xED, 0x25, 0xD1, 0xBB, 0xBC, 0x27, 0x9F, 0x02, 0xA2, 0xA9, 0x11, 0x00, 0x0C, 0xB3, 0x52, 0xC0, 0xBD, 0xE3,
        0x1B, 0x49, 0xC7,
    ];
    const I0_INDEX: [usize; 11] = [18, 22, 23, 0, 5, 19, 32, 31, 10, 21, 30];

    // Load the input into the buffer, each word's bytes the other way round.
    for i in 0..210 {
        let in_word = word(block_in, (i % 64) >> 2);
        buffer1[i] = (in_word >> ((3 - (i % 4)) << 3)) as u8;
    }
    // Next a scrambling. The original takes each index as an unsigned 32-bit number.
    for i in 0..840_u32 {
        let x = buffer1[(i.wrapping_sub(155) % 210) as usize];
        let y = buffer1[(i.wrapping_sub(57) % 210) as usize];
        let z = buffer1[(i.wrapping_sub(13) % 210) as usize];
        let w = buffer1[(i % 210) as usize];
        buffer1[(i % 210) as usize] = rol8(y, 5).wrapping_add(rol8(z, 3) ^ w).wrapping_sub(rol8(x, 7));
    }
    garble::garble(&mut buffer0, &mut buffer1, &mut buffer2, &mut buffer3, &buffer4);
    *key_out = [0xE1; 16];
    // Addition, not XOR; only certain places of buffer3; and place 3 is always 0x3d.
    for i in 0..11 {
        key_out[i] = if i == 3 { 0x3d } else { key_out[i].wrapping_add(buffer3[I0_INDEX[i] * 4]) };
    }
    for i in 0..20 {
        key_out[i % 16] ^= buffer0[i];
    }
    for i in 0..35 {
        key_out[i % 16] ^= buffer2[i];
    }
    for i in 0..210 {
        key_out[i % 16] ^= buffer1[i];
    }
    // A kind of reverse scramble.
    for _ in 0..16 {
        for i in 0..16_u32 {
            let x = key_out[(i.wrapping_sub(7) % 16) as usize];
            let y = key_out[(i % 16) as usize];
            let z = key_out[(i.wrapping_sub(37) % 16) as usize];
            let w = key_out[(i.wrapping_sub(177) % 16) as usize];
            key_out[i as usize] = rol8(x, 1) ^ y ^ rol8(z, 6) ^ rol8(w, 5);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes<const N: usize>(hex: &str) -> [u8; N] {
        assert_eq!(hex.len(), N * 2);
        let mut out = [0_u8; N];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap();
        }
        out
    }

    /// `vectors.txt`: 24 lines of second message, scrambled key and the key the original C
    /// (playfair as in UxPlay fbd040e, compiled with clang) makes of them, six for each mode.
    #[test]
    fn the_port_gives_the_keys_the_original_gives() {
        let vectors = include_str!("vectors.txt");
        let mut modes = [0; 4];
        for line in vectors.lines() {
            let mut parts = line.split(' ');
            let message: [u8; HANDSHAKE_LEN] = bytes(parts.next().unwrap());
            let ekey: [u8; EKEY_LEN] = bytes(parts.next().unwrap());
            let key: [u8; 16] = bytes(parts.next().unwrap());
            let mut fairplay = FairPlay::default();
            fairplay.handshake(&message).unwrap();
            assert_eq!(fairplay.decrypt(&ekey).unwrap(), key, "mode {}", message[12]);
            modes[usize::from(message[12])] += 1;
        }
        assert_eq!(modes, [6; 4]);
    }

    #[test]
    fn the_first_answer_is_the_fixed_message_of_the_mode_asked_for() {
        for mode in 0..4_u8 {
            let mut request = [0_u8; SETUP_LEN];
            request[..4].copy_from_slice(b"FPLY");
            request[4] = 3;
            request[14] = mode;
            let reply = FairPlay::default().setup(&request).unwrap();
            assert_eq!(&reply[..4], b"FPLY");
            assert_eq!(reply[4], 3);
            assert_eq!(reply[13], mode);
        }
    }

    #[test]
    fn the_second_answer_is_the_header_and_the_last_twenty_bytes() {
        let mut request = [0_u8; HANDSHAKE_LEN];
        request[4] = 3;
        for (i, byte) in request.iter_mut().enumerate().skip(16) {
            *byte = i as u8;
        }
        let reply = FairPlay::default().handshake(&request).unwrap();
        assert_eq!(&reply[..12], &[0x46, 0x50, 0x4c, 0x59, 0x03, 0x01, 0x04, 0x00, 0x00, 0x00, 0x00, 0x14]);
        assert_eq!(&reply[12..], &request[144..]);
    }

    #[test]
    fn other_versions_modes_and_a_key_before_the_handshake_are_refused() {
        let mut fairplay = FairPlay::default();
        let mut request = [0_u8; SETUP_LEN];
        request[4] = 2;
        assert_eq!(fairplay.setup(&request), Err(Refused::Unsupported));
        request[4] = 3;
        request[14] = 4;
        assert_eq!(fairplay.setup(&request), Err(Refused::Unsupported));
        let mut message = [0_u8; HANDSHAKE_LEN];
        message[4] = 3;
        message[12] = 9;
        assert_eq!(fairplay.handshake(&message), Err(Refused::Unsupported));
        assert_eq!(fairplay.decrypt(&[0; EKEY_LEN]), Err(Refused::NoHandshake));
        // A refused second message forgets an earlier good one.
        message[12] = 1;
        fairplay.handshake(&message).unwrap();
        message[4] = 1;
        assert!(fairplay.handshake(&message).is_err());
        assert_eq!(fairplay.decrypt(&[0; EKEY_LEN]), Err(Refused::NoHandshake));
    }

    /// Any bytes at all, in every mode: a key comes out and nothing stops.
    #[test]
    fn no_message_makes_it_stop() {
        let mut seed = 0x2545_F491_4F6C_DD1D_u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 24) as u8
        };
        for round in 0..400 {
            let mut message = [0_u8; HANDSHAKE_LEN];
            let mut ekey = [0_u8; EKEY_LEN];
            message.iter_mut().for_each(|byte| *byte = next());
            ekey.iter_mut().for_each(|byte| *byte = next());
            if round % 8 == 0 {
                message = [if round % 16 == 0 { 0 } else { 0xff }; HANDSHAKE_LEN];
            }
            message[4] = 3;
            message[12] = (round % 4) as u8;
            let mut fairplay = FairPlay::default();
            fairplay.handshake(&message).unwrap();
            fairplay.decrypt(&ekey).unwrap();
        }
    }
}
