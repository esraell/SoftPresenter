//! The middle of the FairPlay step's hash (`sap_hash`): playfair's `hand_garble.c`, turned into
//! Rust statement by statement by a script that keeps C's arithmetic: a byte takes part as a
//! signed 32-bit number, the single-letter values are unsigned 32-bit, and each result is cut to
//! a byte where the original stores it in one. Nothing here is written by hand, and it is not
//! meant to be read; playfair's author did not know what it computes either.
//!
//! From playfair as carried in UxPlay's `lib/playfair/` (commit fbd040e), GPL-3.0; see
//! `docs/licences/screen-mirroring.md`.

#![allow(clippy::all, clippy::pedantic, unused_parens, unused_assignments, unused_variables, unused_mut)]

pub(super) fn rol8(input: u8, count: i32) -> u8 {
    let input = i32::from(input);
    ((input.wrapping_shl(count as u32) & 0xff) | ((input & 0xff).wrapping_shr((8 - count) as u32))) as u8
}

fn rol8x(input: u8, count: i32) -> u32 {
    let input = i32::from(input);
    (input.wrapping_shl(count as u32) | input.wrapping_shr((8 - count) as u32)) as u32
}

fn weird_ror8(input: u8, count: i32) -> u32 {
    if count == 0 {
        return 0;
    }
    let input = i32::from(input);
    ((input.wrapping_shr(count as u32) & 0xff) | (input & 0xff).wrapping_shl((8 - count) as u32)) as u32
}

fn weird_rol8(input: u8, count: i32) -> u32 {
    if count == 0 {
        return 0;
    }
    let input = i32::from(input);
    ((input.wrapping_shl(count as u32) & 0xff) | (input & 0xff).wrapping_shr((8 - count) as u32)) as u32
}

fn weird_rol32(input: u8, count: i32) -> u32 {
    if count == 0 {
        return 0;
    }
    let input = i32::from(input);
    (input.wrapping_shl(count as u32) ^ input.wrapping_shr((8 - count) as u32)) as u32
}

#[rustfmt::skip]
pub(super) fn garble(
    buffer0: &mut [u8; 20],
    buffer1: &mut [u8; 210],
    buffer2: &mut [u8; 35],
    buffer3: &mut [u8; 132],
    buffer4: &[u8; 21],
) {
    let (mut tmp, mut tmp2, mut tmp3);
    let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h, mut j, mut k, mut m, mut r, mut s, mut t, mut u, mut v, mut w, mut x, mut y, mut z);
    buffer2[(12_i32) as usize] = (20_i32.wrapping_add(((((((i32::from(buffer1[(64_i32) as usize]) & 92_i32)) | (((i32::from(buffer1[(99_i32) as usize]).wrapping_div(3_i32)) & 35_i32)))) & i32::from(buffer4[(rol8x((i32::from(buffer4[((i32::from(buffer1[(206_i32) as usize]).wrapping_rem(21_i32))) as usize])) as u8, (4_i32) as i32).wrapping_rem(((21_i32) as u32))) as usize]))))) as u8;
    buffer1[(4_i32) as usize] = ((i32::from(buffer1[(99_i32) as usize]).wrapping_div(5_i32)).wrapping_mul((i32::from(buffer1[(99_i32) as usize]).wrapping_div(5_i32))).wrapping_mul(2_i32)) as u8;
    buffer2[(34_i32) as usize] = (184_i32) as u8;
    buffer1[(153_i32) as usize] = ((i32::from(buffer1[(153_i32) as usize]) ^ (i32::from(buffer2[(i32::from(buffer1[(203_i32) as usize]).wrapping_rem(35_i32)) as usize]).wrapping_mul(i32::from(buffer2[(i32::from(buffer1[(203_i32) as usize]).wrapping_rem(35_i32)) as usize])).wrapping_mul(i32::from(buffer1[(190_i32) as usize]))))) as u8;
    buffer0[(3_i32) as usize] = (i32::from(buffer0[(3_i32) as usize]).wrapping_sub((((((i32::from(buffer4[(i32::from(buffer1[(205_i32) as usize]).wrapping_rem(21_i32)) as usize]).wrapping_shr((1_i32) as u32)) & 80_i32)) | 943168_i32)))) as u8;
    buffer0[(16_i32) as usize] = (147_i32) as u8;
    buffer0[(13_i32) as usize] = (98_i32) as u8;
    buffer1[(33_i32) as usize] = (i32::from(buffer1[(33_i32) as usize]).wrapping_sub(((i32::from(buffer4[(i32::from(buffer1[(36_i32) as usize]).wrapping_rem(21_i32)) as usize]) & 246_i32)))) as u8;
    tmp2 = (i32::from(buffer2[(i32::from(buffer1[(67_i32) as usize]).wrapping_rem(35_i32)) as usize])) as u32;
    buffer2[(12_i32) as usize] = (7_i32) as u8;
    tmp = (i32::from(buffer0[(i32::from(buffer1[(181_i32) as usize]).wrapping_rem(20_i32)) as usize])) as u32;
    buffer1[(2_i32) as usize] = (i32::from(buffer1[(2_i32) as usize]).wrapping_sub(3136_i32)) as u8;
    buffer0[(19_i32) as usize] = (i32::from(buffer4[(i32::from(buffer1[(58_i32) as usize]).wrapping_rem(21_i32)) as usize])) as u8;
    buffer3[(0_i32) as usize] = (92_i32.wrapping_sub(i32::from(buffer2[(i32::from(buffer1[(32_i32) as usize]).wrapping_rem(35_i32)) as usize]))) as u8;
    buffer3[(4_i32) as usize] = (i32::from(buffer2[(i32::from(buffer1[(15_i32) as usize]).wrapping_rem(35_i32)) as usize]).wrapping_add(158_i32)) as u8;
    buffer1[(34_i32) as usize] = (i32::from(buffer1[(34_i32) as usize]).wrapping_add((i32::from(buffer4[((((i32::from(buffer2[(i32::from(buffer1[(15_i32) as usize]).wrapping_rem(35_i32)) as usize]).wrapping_add(158_i32)) & 255_i32)).wrapping_rem(21_i32)) as usize]).wrapping_div(5_i32)))) as u8;
    buffer0[(19_i32) as usize] = (((i32::from(buffer0[(19_i32) as usize])) as u32).wrapping_add(0xfffffee6_u32.wrapping_sub((((((i32::from(buffer0[(i32::from(buffer3[(4_i32) as usize]).wrapping_rem(20_i32)) as usize]).wrapping_shr((1_i32) as u32)) & 102_i32))) as u32)))) as u8;
    buffer1[(15_i32) as usize] = (((3_i32.wrapping_mul(((((i32::from(buffer1[(72_i32) as usize]).wrapping_shr((((i32::from(buffer4[(i32::from(buffer1[(190_i32) as usize]).wrapping_rem(21_i32)) as usize]) & 7_i32))) as u32)) ^ (i32::from(buffer1[(72_i32) as usize]).wrapping_shl(((((7_i32.wrapping_sub((i32::from(buffer4[(i32::from(buffer1[(190_i32) as usize]).wrapping_rem(21_i32)) as usize]).wrapping_sub(1_i32))) & 7_i32)))) as u32)))).wrapping_sub((3_i32.wrapping_mul(i32::from(buffer4[(i32::from(buffer1[(126_i32) as usize]).wrapping_rem(21_i32)) as usize]))))))) ^ i32::from(buffer1[(15_i32) as usize]))) as u8;
    buffer0[(15_i32) as usize] = ((i32::from(buffer0[(15_i32) as usize]) ^ i32::from(buffer2[(i32::from(buffer1[(181_i32) as usize]).wrapping_rem(35_i32)) as usize]).wrapping_mul(i32::from(buffer2[(i32::from(buffer1[(181_i32) as usize]).wrapping_rem(35_i32)) as usize])).wrapping_mul(i32::from(buffer2[(i32::from(buffer1[(181_i32) as usize]).wrapping_rem(35_i32)) as usize])))) as u8;
    buffer2[(4_i32) as usize] = ((i32::from(buffer2[(4_i32) as usize]) ^ i32::from(buffer1[(202_i32) as usize]).wrapping_div(3_i32))) as u8;
    a = (92_i32.wrapping_sub(i32::from(buffer0[(i32::from(buffer3[(0_i32) as usize]).wrapping_rem(20_i32)) as usize]))) as u32;
    e = (((((a & ((198_i32) as u32))) | (((((!i32::from(buffer1[(105_i32) as usize])) & 198_i32))) as u32)) | ((a & ((((!i32::from(buffer1[(105_i32) as usize])))) as u32))))) as u32;
    buffer2[(1_i32) as usize] = (((i32::from(buffer2[(1_i32) as usize])) as u32).wrapping_add((e.wrapping_mul(e).wrapping_mul(e)))) as u8;
    buffer0[(19_i32) as usize] = ((i32::from(buffer0[(19_i32) as usize]) ^ (((224_i32 | ((i32::from(buffer4[(i32::from(buffer1[(92_i32) as usize]).wrapping_rem(21_i32)) as usize]) & 27_i32)))).wrapping_mul(i32::from(buffer2[(i32::from(buffer1[(41_i32) as usize]).wrapping_rem(35_i32)) as usize]))).wrapping_div(3_i32))) as u8;
    buffer1[(140_i32) as usize] = (((i32::from(buffer1[(140_i32) as usize])) as u32).wrapping_add(weird_ror8((92_i32) as u8, ((i32::from(buffer1[(5_i32) as usize]) & 7_i32)) as i32))) as u8;
    buffer2[(12_i32) as usize] = (i32::from(buffer2[(12_i32) as usize]).wrapping_add((((((((((!i32::from(buffer1[(4_i32) as usize]))) ^ i32::from(buffer2[(i32::from(buffer1[(12_i32) as usize]).wrapping_rem(35_i32)) as usize]))) | i32::from(buffer1[(182_i32) as usize]))) & 192_i32)) | ((((((!i32::from(buffer1[(4_i32) as usize]))) ^ i32::from(buffer2[(i32::from(buffer1[(12_i32) as usize]).wrapping_rem(35_i32)) as usize]))) & i32::from(buffer1[(182_i32) as usize])))))) as u8;
    buffer1[(36_i32) as usize] = (i32::from(buffer1[(36_i32) as usize]).wrapping_add(125_i32)) as u8;
    buffer1[(124_i32) as usize] = (rol8x(((((((((74_i32 & i32::from(buffer1[(138_i32) as usize]))) | ((((74_i32 | i32::from(buffer1[(138_i32) as usize]))) & i32::from(buffer0[(15_i32) as usize]))))) & i32::from(buffer0[(i32::from(buffer1[(43_i32) as usize]).wrapping_rem(20_i32)) as usize]))) | (((((((74_i32 & i32::from(buffer1[(138_i32) as usize]))) | ((((74_i32 | i32::from(buffer1[(138_i32) as usize]))) & i32::from(buffer0[(15_i32) as usize])))) | i32::from(buffer0[(i32::from(buffer1[(43_i32) as usize]).wrapping_rem(20_i32)) as usize]))) & 95_i32)))) as u8, (4_i32) as i32)) as u8;
    buffer3[(8_i32) as usize] = (((((((((i32::from(buffer0[(i32::from(buffer3[(4_i32) as usize]).wrapping_rem(20_i32)) as usize]) & 95_i32))) & (((i32::from(buffer4[(i32::from(buffer1[(68_i32) as usize]).wrapping_rem(21_i32)) as usize]) & 46_i32)).wrapping_shl((1_i32) as u32)))) | 16_i32)) ^ 92_i32)) as u8;
    a = (i32::from(buffer1[(177_i32) as usize]).wrapping_add(i32::from(buffer4[(i32::from(buffer1[(79_i32) as usize]).wrapping_rem(21_i32)) as usize]))) as u32;
    d = (((((((a.wrapping_shr((1_i32) as u32)) | ((((3_i32.wrapping_mul(i32::from(buffer1[(148_i32) as usize]))).wrapping_div(5_i32))) as u32))) & ((i32::from(buffer2[(1_i32) as usize])) as u32))) | (((a.wrapping_shr((1_i32) as u32)) & ((((3_i32.wrapping_mul(i32::from(buffer1[(148_i32) as usize]))).wrapping_div(5_i32))) as u32))))) as u32;
    buffer3[(12_i32) as usize] = (((((34_i32.wrapping_neg()) as u32).wrapping_sub(d)))) as u8;
    a = (8_i32.wrapping_sub((((i32::from(buffer2[(22_i32) as usize]) & 7_i32))))) as u32;
    b = ((i32::from(buffer1[(33_i32) as usize]).wrapping_shr((((a & ((7_i32) as u32)))) as u32))) as u32;
    c = (i32::from(buffer1[(33_i32) as usize]).wrapping_shl((((i32::from(buffer2[(22_i32) as usize]) & 7_i32))) as u32)) as u32;
    buffer2[(16_i32) as usize] = (((i32::from(buffer2[(16_i32) as usize])) as u32).wrapping_add((((((((i32::from(buffer2[(i32::from(buffer3[(0_i32) as usize]).wrapping_rem(35_i32)) as usize]) & 159_i32)) | i32::from(buffer0[(i32::from(buffer3[(4_i32) as usize]).wrapping_rem(20_i32)) as usize])) | 8_i32))) as u32).wrapping_sub(((((b ^ c)) | ((128_i32) as u32)))))) as u8;
    buffer0[(14_i32) as usize] = ((i32::from(buffer0[(14_i32) as usize]) ^ i32::from(buffer2[(i32::from(buffer3[(12_i32) as usize]).wrapping_rem(35_i32)) as usize]))) as u8;
    a = (weird_rol8((i32::from(buffer4[(i32::from(buffer0[(i32::from(buffer1[(201_i32) as usize]).wrapping_rem(20_i32)) as usize]).wrapping_rem(21_i32)) as usize])) as u8, ((((i32::from(buffer2[(i32::from(buffer1[(112_i32) as usize]).wrapping_rem(35_i32)) as usize]).wrapping_shl((1_i32) as u32)) & 7_i32))) as i32)) as u32;
    d = ((((i32::from(buffer0[(i32::from(buffer1[(208_i32) as usize]).wrapping_rem(20_i32)) as usize]) & 131_i32)) | ((i32::from(buffer0[(i32::from(buffer1[(164_i32) as usize]).wrapping_rem(20_i32)) as usize]) & 124_i32)))) as u32;
    buffer1[(19_i32) as usize] = (((i32::from(buffer1[(19_i32) as usize])) as u32).wrapping_add((((a & (d.wrapping_div(((5_i32) as u32))))) | ((((a | (d.wrapping_div(((5_i32) as u32))))) & ((37_i32) as u32)))))) as u8;
    buffer2[(8_i32) as usize] = (weird_ror8((140_i32) as u8, ((((i32::from(buffer4[(i32::from(buffer1[(45_i32) as usize]).wrapping_rem(21_i32)) as usize]).wrapping_add(92_i32)).wrapping_mul((i32::from(buffer4[(i32::from(buffer1[(45_i32) as usize]).wrapping_rem(21_i32)) as usize]).wrapping_add(92_i32)))) & 7_i32)) as i32)) as u8;
    buffer1[(190_i32) as usize] = (56_i32) as u8;
    buffer2[(8_i32) as usize] = ((i32::from(buffer2[(8_i32) as usize]) ^ i32::from(buffer3[(0_i32) as usize]))) as u8;
    buffer1[(53_i32) as usize] = ((!(((i32::from(buffer0[(i32::from(buffer1[(83_i32) as usize]).wrapping_rem(20_i32)) as usize]) | 204_i32)).wrapping_div(5_i32)))) as u8;
    buffer0[(13_i32) as usize] = (i32::from(buffer0[(13_i32) as usize]).wrapping_add(i32::from(buffer0[(i32::from(buffer1[(41_i32) as usize]).wrapping_rem(20_i32)) as usize]))) as u8;
    buffer0[(10_i32) as usize] = (((((i32::from(buffer2[(i32::from(buffer3[(0_i32) as usize]).wrapping_rem(35_i32)) as usize]) & i32::from(buffer1[(2_i32) as usize]))) | ((((i32::from(buffer2[(i32::from(buffer3[(0_i32) as usize]).wrapping_rem(35_i32)) as usize]) | i32::from(buffer1[(2_i32) as usize]))) & i32::from(buffer3[(12_i32) as usize]))))).wrapping_div(15_i32)) as u8;
    a = ((((((((56_i32 | ((i32::from(buffer4[(i32::from(buffer1[(2_i32) as usize]).wrapping_rem(21_i32)) as usize]) & 68_i32)))) | i32::from(buffer2[(i32::from(buffer3[(8_i32) as usize]).wrapping_rem(35_i32)) as usize]))) & 42_i32)) | ((((((i32::from(buffer4[(i32::from(buffer1[(2_i32) as usize]).wrapping_rem(21_i32)) as usize]) & 68_i32)) | 56_i32)) & i32::from(buffer2[(i32::from(buffer3[(8_i32) as usize]).wrapping_rem(35_i32)) as usize]))))) as u32;
    buffer3[(16_i32) as usize] = ((a.wrapping_mul(a)).wrapping_add(((110_i32) as u32))) as u8;
    buffer3[(20_i32) as usize] = (202_i32.wrapping_sub(i32::from(buffer3[(16_i32) as usize]))) as u8;
    buffer3[(24_i32) as usize] = (i32::from(buffer1[(151_i32) as usize])) as u8;
    buffer2[(13_i32) as usize] = ((i32::from(buffer2[(13_i32) as usize]) ^ i32::from(buffer4[(i32::from(buffer3[(0_i32) as usize]).wrapping_rem(21_i32)) as usize]))) as u8;
    b = (((((i32::from(buffer2[(i32::from(buffer1[(179_i32) as usize]).wrapping_rem(35_i32)) as usize]).wrapping_sub(38_i32)) & 177_i32)) | ((i32::from(buffer3[(12_i32) as usize]) & 177_i32)))) as u32;
    c = ((((i32::from(buffer2[(i32::from(buffer1[(179_i32) as usize]).wrapping_rem(35_i32)) as usize]).wrapping_sub(38_i32))) & i32::from(buffer3[(12_i32) as usize]))) as u32;
    buffer3[(28_i32) as usize] = (((30_i32) as u32).wrapping_add((((b | c)).wrapping_mul(((b | c)))))) as u8;
    buffer3[(32_i32) as usize] = (i32::from(buffer3[(28_i32) as usize]).wrapping_add(62_i32)) as u8;
    a = (((((i32::from(buffer3[(20_i32) as usize]).wrapping_add(((i32::from(buffer3[(0_i32) as usize]) & 74_i32)))) | (!i32::from(buffer4[(i32::from(buffer3[(0_i32) as usize]).wrapping_rem(21_i32)) as usize])))) & 121_i32)) as u32;
    b = ((((i32::from(buffer3[(20_i32) as usize]).wrapping_add(((i32::from(buffer3[(0_i32) as usize]) & 74_i32)))) & (!i32::from(buffer4[(i32::from(buffer3[(0_i32) as usize]).wrapping_rem(21_i32)) as usize]))))) as u32;
    tmp3 = (((a | b))) as u32;
    c = ((((((((((a | b)) ^ 0xffffffa6_u32)) | ((i32::from(buffer3[(0_i32) as usize])) as u32))) & ((4_i32) as u32))) | ((((((a | b)) ^ 0xffffffa6_u32)) & ((i32::from(buffer3[(0_i32) as usize])) as u32))))) as u32;
    buffer1[(47_i32) as usize] = (((((i32::from(buffer2[(i32::from(buffer1[(89_i32) as usize]).wrapping_rem(35_i32)) as usize])) as u32).wrapping_add(c)) ^ ((i32::from(buffer1[(47_i32) as usize])) as u32))) as u8;
    buffer3[(36_i32) as usize] = (((((((i32::from(rol8((((tmp & ((179_i32) as u32))).wrapping_add(((68_i32) as u32))) as u8, (2_i32) as i32)) & i32::from(buffer0[(3_i32) as usize])))) as u32) | ((tmp2 & (((!i32::from(buffer0[(3_i32) as usize]))) as u32))))).wrapping_sub(((15_i32) as u32))) as u8;
    buffer1[(123_i32) as usize] = ((i32::from(buffer1[(123_i32) as usize]) ^ 221_i32)) as u8;
    a = (((i32::from(buffer4[(i32::from(buffer3[(0_i32) as usize]).wrapping_rem(21_i32)) as usize])).wrapping_div(3_i32)).wrapping_sub(i32::from(buffer2[(i32::from(buffer3[(4_i32) as usize]).wrapping_rem(35_i32)) as usize]))) as u32;
    c = (((((((i32::from(buffer3[(0_i32) as usize]) & 163_i32)).wrapping_add(92_i32)) & 246_i32)) | ((i32::from(buffer3[(0_i32) as usize]) & 92_i32)))) as u32;
    e = ((((((c | ((i32::from(buffer3[(24_i32) as usize])) as u32))) & ((54_i32) as u32))) | ((c & ((i32::from(buffer3[(24_i32) as usize])) as u32))))) as u32;
    buffer3[(40_i32) as usize] = (a.wrapping_sub(e)) as u8;
    buffer3[(44_i32) as usize] = (((tmp3 ^ ((81_i32) as u32)) ^ ((((((i32::from(buffer3[(0_i32) as usize]).wrapping_shr((1_i32) as u32)) & 101_i32)).wrapping_add(26_i32))) as u32))) as u8;
    buffer3[(48_i32) as usize] = ((i32::from(buffer2[(i32::from(buffer3[(4_i32) as usize]).wrapping_rem(35_i32)) as usize]) & 27_i32)) as u8;
    buffer3[(52_i32) as usize] = (27_i32) as u8;
    buffer3[(56_i32) as usize] = (199_i32) as u8;
    buffer3[(64_i32) as usize] = (i32::from(buffer3[(4_i32) as usize]).wrapping_add(((((((((((((((i32::from(buffer3[(40_i32) as usize]) | i32::from(buffer3[(24_i32) as usize]))) & 177_i32)) | ((i32::from(buffer3[(40_i32) as usize]) & i32::from(buffer3[(24_i32) as usize]))))) & (((((((i32::from(buffer4[(i32::from(buffer3[(0_i32) as usize]).wrapping_rem(20_i32)) as usize]) & 177_i32)) | 176_i32))) | (((i32::from(buffer4[(i32::from(buffer3[(0_i32) as usize]).wrapping_rem(21_i32)) as usize])) & (!3_i32))))))) | ((((((((i32::from(buffer3[(40_i32) as usize]) & i32::from(buffer3[(24_i32) as usize]))) | ((((i32::from(buffer3[(40_i32) as usize]) | i32::from(buffer3[(24_i32) as usize]))) & 177_i32)))) & 199_i32)) | (((((((i32::from(buffer4[(i32::from(buffer3[(0_i32) as usize]).wrapping_rem(21_i32)) as usize]) & 1_i32)).wrapping_add(176_i32)) | ((i32::from(buffer4[(i32::from(buffer3[(0_i32) as usize]).wrapping_rem(21_i32)) as usize]) & (!3_i32))))) & i32::from(buffer3[(56_i32) as usize]))))))) & ((!i32::from(buffer3[(52_i32) as usize]))))) | i32::from(buffer3[(48_i32) as usize]))))) as u8;
    buffer2[(33_i32) as usize] = ((i32::from(buffer2[(33_i32) as usize]) ^ i32::from(buffer1[(26_i32) as usize]))) as u8;
    buffer1[(106_i32) as usize] = ((i32::from(buffer1[(106_i32) as usize]) ^ (i32::from(buffer3[(20_i32) as usize]) ^ 133_i32))) as u8;
    buffer2[(30_i32) as usize] = ((((i32::from(buffer3[(64_i32) as usize]).wrapping_div(3_i32)).wrapping_sub(((275_i32 | ((i32::from(buffer3[(0_i32) as usize]) & 247_i32)))))) ^ i32::from(buffer0[(i32::from(buffer1[(122_i32) as usize]).wrapping_rem(20_i32)) as usize]))) as u8;
    buffer1[(22_i32) as usize] = ((((i32::from(buffer2[(i32::from(buffer1[(90_i32) as usize]).wrapping_rem(35_i32)) as usize]) & 95_i32)) | 68_i32)) as u8;
    a = ((((i32::from(buffer4[(i32::from(buffer3[(36_i32) as usize]).wrapping_rem(21_i32)) as usize]) & 184_i32)) | ((i32::from(buffer2[(i32::from(buffer3[(44_i32) as usize]).wrapping_rem(35_i32)) as usize]) & (!184_i32))))) as u32;
    buffer2[(18_i32) as usize] = (((i32::from(buffer2[(18_i32) as usize])) as u32).wrapping_add(((a.wrapping_mul(a).wrapping_mul(a)).wrapping_shr((1_i32) as u32)))) as u8;
    buffer2[(5_i32) as usize] = (i32::from(buffer2[(5_i32) as usize]).wrapping_sub(i32::from(buffer4[(i32::from(buffer1[(92_i32) as usize]).wrapping_rem(21_i32)) as usize]))) as u8;
    a = ((((((((i32::from(buffer1[(41_i32) as usize]) & (!24_i32))) | ((i32::from(buffer2[(i32::from(buffer1[(183_i32) as usize]).wrapping_rem(35_i32)) as usize]) & 24_i32)))) & (i32::from(buffer3[(16_i32) as usize]).wrapping_add(53_i32)))) | ((i32::from(buffer3[(20_i32) as usize]) & i32::from(buffer2[(i32::from(buffer3[(20_i32) as usize]).wrapping_rem(35_i32)) as usize]))))) as u32;
    b = ((((i32::from(buffer1[(17_i32) as usize]) & ((!i32::from(buffer3[(44_i32) as usize]))))) | ((i32::from(buffer0[(i32::from(buffer1[(59_i32) as usize]).wrapping_rem(20_i32)) as usize]) & i32::from(buffer3[(44_i32) as usize]))))) as u32;
    buffer2[(18_i32) as usize] = ((((i32::from(buffer2[(18_i32) as usize])) as u32) ^ (a.wrapping_mul(b)))) as u8;
    a = ((weird_ror8((i32::from(buffer1[(11_i32) as usize])) as u8, ((i32::from(buffer2[(i32::from(buffer1[(28_i32) as usize]).wrapping_rem(35_i32)) as usize]) & 7_i32)) as i32) & ((7_i32) as u32))) as u32;
    b = ((((((((i32::from(buffer0[(i32::from(buffer1[(93_i32) as usize]).wrapping_rem(20_i32)) as usize]) & (!i32::from(buffer0[(14_i32) as usize])))) | ((i32::from(buffer0[(14_i32) as usize]) & 150_i32)))) & (!28_i32))) | ((i32::from(buffer1[(7_i32) as usize]) & 28_i32)))) as u32;
    buffer2[(22_i32) as usize] = ((((((((((b | weird_rol8((i32::from(buffer2[(i32::from(buffer3[(0_i32) as usize]).wrapping_rem(35_i32)) as usize])) as u8, (a) as i32))) & ((i32::from(buffer2[(33_i32) as usize])) as u32))) | ((b & weird_rol8((i32::from(buffer2[(i32::from(buffer3[(0_i32) as usize]).wrapping_rem(35_i32)) as usize])) as u8, (a) as i32))))).wrapping_add(((74_i32) as u32))) & ((255_i32) as u32)))) as u8;
    a = (i32::from(buffer4[(((i32::from(buffer0[(i32::from(buffer1[(39_i32) as usize]).wrapping_rem(20_i32)) as usize]) ^ 217_i32)).wrapping_rem(21_i32)) as usize])) as u32;
    buffer0[(15_i32) as usize] = (((i32::from(buffer0[(15_i32) as usize])) as u32).wrapping_sub((((((((((((i32::from(buffer3[(20_i32) as usize]) | i32::from(buffer3[(0_i32) as usize]))) & 214_i32)) | ((i32::from(buffer3[(20_i32) as usize]) & i32::from(buffer3[(0_i32) as usize])))))) as u32) & a)) | (((((((((((i32::from(buffer3[(20_i32) as usize]) | i32::from(buffer3[(0_i32) as usize]))) & 214_i32)) | ((i32::from(buffer3[(20_i32) as usize]) & i32::from(buffer3[(0_i32) as usize]))))) as u32) | a)) & ((i32::from(buffer3[(32_i32) as usize])) as u32)))))) as u8;
    b = (((((((((i32::from(buffer2[(i32::from(buffer1[(57_i32) as usize]).wrapping_rem(35_i32)) as usize]) & i32::from(buffer0[(i32::from(buffer3[(64_i32) as usize]).wrapping_rem(20_i32)) as usize]))) | ((((i32::from(buffer0[(i32::from(buffer3[(64_i32) as usize]).wrapping_rem(20_i32)) as usize]) | i32::from(buffer2[(i32::from(buffer1[(57_i32) as usize]).wrapping_rem(35_i32)) as usize]))) & 95_i32))) | ((i32::from(buffer3[(64_i32) as usize]) & 45_i32))) | 82_i32)) & 32_i32))) as u32;
    c = ((((((i32::from(buffer2[(i32::from(buffer1[(57_i32) as usize]).wrapping_rem(35_i32)) as usize]) & i32::from(buffer0[(i32::from(buffer3[(64_i32) as usize]).wrapping_rem(20_i32)) as usize]))) | ((((i32::from(buffer2[(i32::from(buffer1[(57_i32) as usize]).wrapping_rem(35_i32)) as usize]) | i32::from(buffer0[(i32::from(buffer3[(64_i32) as usize]).wrapping_rem(20_i32)) as usize]))) & 95_i32)))) & ((((i32::from(buffer3[(64_i32) as usize]) & 45_i32)) | 82_i32)))) as u32;
    d = (((((((((i32::from(buffer3[(0_i32) as usize]).wrapping_div(3_i32)).wrapping_sub(((i32::from(buffer3[(64_i32) as usize]) | i32::from(buffer1[(22_i32) as usize])))))) ^ (i32::from(buffer3[(28_i32) as usize]).wrapping_add(62_i32)))) as u32) ^ (((b | c)))))) as u32;
    t = (i32::from(buffer0[(((d & ((255_i32) as u32))).wrapping_rem(((20_i32) as u32))) as usize])) as u32;
    buffer3[(68_i32) as usize] = (((i32::from(buffer0[(i32::from(buffer1[(99_i32) as usize]).wrapping_rem(20_i32)) as usize]).wrapping_mul(i32::from(buffer0[(i32::from(buffer1[(99_i32) as usize]).wrapping_rem(20_i32)) as usize])).wrapping_mul(i32::from(buffer0[(i32::from(buffer1[(99_i32) as usize]).wrapping_rem(20_i32)) as usize])).wrapping_mul(i32::from(buffer0[(i32::from(buffer1[(99_i32) as usize]).wrapping_rem(20_i32)) as usize]))) | i32::from(buffer2[(i32::from(buffer3[(64_i32) as usize]).wrapping_rem(35_i32)) as usize]))) as u8;
    u = (i32::from(buffer0[(i32::from(buffer1[(50_i32) as usize]).wrapping_rem(20_i32)) as usize])) as u32;
    w = (i32::from(buffer2[(i32::from(buffer1[(138_i32) as usize]).wrapping_rem(35_i32)) as usize])) as u32;
    x = (i32::from(buffer4[(i32::from(buffer1[(39_i32) as usize]).wrapping_rem(21_i32)) as usize])) as u32;
    y = (i32::from(buffer0[(i32::from(buffer1[(4_i32) as usize]).wrapping_rem(20_i32)) as usize])) as u32;
    z = (i32::from(buffer4[(i32::from(buffer1[(202_i32) as usize]).wrapping_rem(21_i32)) as usize])) as u32;
    v = (i32::from(buffer0[(i32::from(buffer1[(151_i32) as usize]).wrapping_rem(20_i32)) as usize])) as u32;
    s = (i32::from(buffer2[(i32::from(buffer1[(14_i32) as usize]).wrapping_rem(35_i32)) as usize])) as u32;
    r = (i32::from(buffer0[(i32::from(buffer1[(145_i32) as usize]).wrapping_rem(20_i32)) as usize])) as u32;
    a = ((((i32::from(buffer2[(i32::from(buffer3[(68_i32) as usize]).wrapping_rem(35_i32)) as usize]) & i32::from(buffer0[(i32::from(buffer1[(209_i32) as usize]).wrapping_rem(20_i32)) as usize]))) | ((((i32::from(buffer2[(i32::from(buffer3[(68_i32) as usize]).wrapping_rem(35_i32)) as usize]) | i32::from(buffer0[(i32::from(buffer1[(209_i32) as usize]).wrapping_rem(20_i32)) as usize]))) & 24_i32)))) as u32;
    b = (weird_rol8((i32::from(buffer4[(i32::from(buffer1[(127_i32) as usize]).wrapping_rem(21_i32)) as usize])) as u8, ((i32::from(buffer2[(i32::from(buffer3[(68_i32) as usize]).wrapping_rem(35_i32)) as usize]) & 7_i32)) as i32)) as u32;
    c = ((((a & ((i32::from(buffer0[(10_i32) as usize])) as u32))) | ((b & (((!i32::from(buffer0[(10_i32) as usize]))) as u32))))) as u32;
    d = ((7_i32 ^ (i32::from(buffer4[(i32::from(buffer2[(i32::from(buffer3[(36_i32) as usize]).wrapping_rem(35_i32)) as usize]).wrapping_rem(21_i32)) as usize]).wrapping_shl((1_i32) as u32)))) as u32;
    buffer3[(72_i32) as usize] = ((((c & ((71_i32) as u32))) | ((d & (((!71_i32)) as u32))))) as u8;
    buffer2[(2_i32) as usize] = (i32::from(buffer2[(2_i32) as usize]).wrapping_add(((((((i32::from(buffer0[(i32::from(buffer3[(20_i32) as usize]).wrapping_rem(20_i32)) as usize]).wrapping_shl((1_i32) as u32)) & 159_i32)) | ((i32::from(buffer4[(i32::from(buffer1[(190_i32) as usize]).wrapping_rem(21_i32)) as usize]) & (!159_i32))))) & ((((((((i32::from(buffer4[(i32::from(buffer3[(64_i32) as usize]).wrapping_rem(21_i32)) as usize]) & 110_i32)) | ((i32::from(buffer0[(i32::from(buffer1[(25_i32) as usize]).wrapping_rem(20_i32)) as usize]) & (!110_i32))))) & (!150_i32))) | ((i32::from(buffer1[(25_i32) as usize]) & 150_i32))))))) as u8;
    buffer2[(14_i32) as usize] = (i32::from(buffer2[(14_i32) as usize]).wrapping_sub((((((i32::from(buffer2[(i32::from(buffer3[(20_i32) as usize]).wrapping_rem(35_i32)) as usize]) & ((i32::from(buffer3[(72_i32) as usize]) ^ i32::from(buffer2[(i32::from(buffer1[(100_i32) as usize]).wrapping_rem(35_i32)) as usize]))))) & (!34_i32))) | ((i32::from(buffer1[(97_i32) as usize]) & 34_i32))))) as u8;
    buffer0[(17_i32) as usize] = (115_i32) as u8;
    buffer1[(23_i32) as usize] = ((i32::from(buffer1[(23_i32) as usize]) ^ (((((((((((i32::from(buffer4[(i32::from(buffer1[(17_i32) as usize]).wrapping_rem(21_i32)) as usize]) | i32::from(buffer0[(i32::from(buffer3[(20_i32) as usize]).wrapping_rem(20_i32)) as usize]))) & i32::from(buffer3[(72_i32) as usize]))) | ((i32::from(buffer4[(i32::from(buffer1[(17_i32) as usize]).wrapping_rem(21_i32)) as usize]) & i32::from(buffer0[(i32::from(buffer3[(20_i32) as usize]).wrapping_rem(20_i32)) as usize]))))) & (i32::from(buffer1[(50_i32) as usize]).wrapping_div(3_i32)))) | (((((((((i32::from(buffer4[(i32::from(buffer1[(17_i32) as usize]).wrapping_rem(21_i32)) as usize]) | i32::from(buffer0[(i32::from(buffer3[(20_i32) as usize]).wrapping_rem(20_i32)) as usize]))) & i32::from(buffer3[(72_i32) as usize]))) | ((i32::from(buffer4[(i32::from(buffer1[(17_i32) as usize]).wrapping_rem(21_i32)) as usize]) & i32::from(buffer0[(i32::from(buffer3[(20_i32) as usize]).wrapping_rem(20_i32)) as usize])))) | (i32::from(buffer1[(50_i32) as usize]).wrapping_div(3_i32)))) & 246_i32)))).wrapping_shl((1_i32) as u32)))) as u8;
    buffer0[(13_i32) as usize] = ((((((((((((i32::from(buffer0[(i32::from(buffer3[(40_i32) as usize]).wrapping_rem(20_i32)) as usize]) | i32::from(buffer1[(10_i32) as usize]))) & 82_i32)) | ((i32::from(buffer0[(i32::from(buffer3[(40_i32) as usize]).wrapping_rem(20_i32)) as usize]) & i32::from(buffer1[(10_i32) as usize]))))) & 209_i32)) | (((i32::from(buffer0[(i32::from(buffer1[(39_i32) as usize]).wrapping_rem(20_i32)) as usize]).wrapping_shl((1_i32) as u32)) & 46_i32)))).wrapping_shr((1_i32) as u32))) as u8;
    buffer2[(33_i32) as usize] = (i32::from(buffer2[(33_i32) as usize]).wrapping_sub((i32::from(buffer1[(113_i32) as usize]) & 9_i32))) as u8;
    buffer2[(28_i32) as usize] = (i32::from(buffer2[(28_i32) as usize]).wrapping_sub((((((((2_i32 | ((i32::from(buffer1[(110_i32) as usize]) & 222_i32)))).wrapping_shr((1_i32) as u32)) & (!223_i32))) | ((i32::from(buffer3[(20_i32) as usize]) & 223_i32)))))) as u8;
    j = (weird_rol8((((v | z))) as u8, (((u & ((7_i32) as u32)))) as i32)) as u32;
    a = ((((((i32::from(buffer2[(16_i32) as usize])) as u32) & t)) | ((w & ((((!i32::from(buffer2[(16_i32) as usize])))) as u32))))) as u32;
    b = ((((((i32::from(buffer1[(33_i32) as usize]) & 17_i32))) as u32) | ((x & (((!17_i32)) as u32))))) as u32;
    e = ((((((y | ((a.wrapping_add(b)).wrapping_div(((5_i32) as u32))))) & ((147_i32) as u32))) | ((y & ((a.wrapping_add(b)).wrapping_div(((5_i32) as u32))))))) as u32;
    m = ((((i32::from(buffer3[(40_i32) as usize]) & i32::from(buffer4[((((((i32::from(buffer3[(8_i32) as usize])) as u32).wrapping_add(j).wrapping_add(e)) & ((255_i32) as u32))).wrapping_rem(((21_i32) as u32))) as usize]))) | ((((i32::from(buffer3[(40_i32) as usize]) | i32::from(buffer4[((((((i32::from(buffer3[(8_i32) as usize])) as u32).wrapping_add(j).wrapping_add(e)) & ((255_i32) as u32))).wrapping_rem(((21_i32) as u32))) as usize]))) & i32::from(buffer2[(23_i32) as usize]))))) as u32;
    buffer0[(15_i32) as usize] = ((((((((((i32::from(buffer4[(i32::from(buffer3[(20_i32) as usize]).wrapping_rem(21_i32)) as usize]).wrapping_sub(48_i32)) & ((!i32::from(buffer1[(184_i32) as usize]))))) | (((i32::from(buffer4[(i32::from(buffer3[(20_i32) as usize]).wrapping_rem(21_i32)) as usize]).wrapping_sub(48_i32)) & 189_i32))) | ((189_i32 & (!i32::from(buffer1[(184_i32) as usize]))))))) as u32) & (m.wrapping_mul(m).wrapping_mul(m)))) as u8;
    buffer2[(22_i32) as usize] = (i32::from(buffer2[(22_i32) as usize]).wrapping_add(i32::from(buffer1[(183_i32) as usize]))) as u8;
    buffer3[(76_i32) as usize] = (((3_i32.wrapping_mul(i32::from(buffer4[(i32::from(buffer1[(1_i32) as usize]).wrapping_rem(21_i32)) as usize]))) ^ i32::from(buffer3[(0_i32) as usize]))) as u8;
    a = (i32::from(buffer2[((((((i32::from(buffer3[(8_i32) as usize])) as u32).wrapping_add((j.wrapping_add(e)))) & ((255_i32) as u32))).wrapping_rem(((35_i32) as u32))) as usize])) as u32;
    f = ((((((((i32::from(buffer4[(i32::from(buffer1[(178_i32) as usize]).wrapping_rem(21_i32)) as usize])) as u32) & a)) | ((((((i32::from(buffer4[(i32::from(buffer1[(178_i32) as usize]).wrapping_rem(21_i32)) as usize])) as u32) | a)) & ((209_i32) as u32))))).wrapping_mul(((i32::from(buffer0[(i32::from(buffer1[(13_i32) as usize]).wrapping_rem(20_i32)) as usize])) as u32))).wrapping_mul((((i32::from(buffer4[(i32::from(buffer1[(26_i32) as usize]).wrapping_rem(21_i32)) as usize]).wrapping_shr((1_i32) as u32))) as u32))) as u32;
    g = ((f.wrapping_add(((1933574137_i32) as u32))).wrapping_mul(((198_i32) as u32)).wrapping_sub(((((f.wrapping_add(((1933574137_i32) as u32))).wrapping_mul(((396_i32) as u32)).wrapping_add(((212_i32) as u32))) & ((212_i32) as u32)))).wrapping_add(((85_i32) as u32))) as u32;
    buffer3[(80_i32) as usize] = (((i32::from(buffer3[(36_i32) as usize])) as u32).wrapping_add(((g ^ ((148_i32) as u32)))).wrapping_add((((g ^ ((107_i32) as u32))).wrapping_shl((1_i32) as u32))).wrapping_sub(((127_i32) as u32))) as u8;
    buffer3[(84_i32) as usize] = (((((i32::from(buffer2[(i32::from(buffer3[(64_i32) as usize]).wrapping_rem(35_i32)) as usize])) & 245_i32)) | ((i32::from(buffer2[(i32::from(buffer3[(20_i32) as usize]).wrapping_rem(35_i32)) as usize]) & 10_i32)))) as u8;
    a = ((i32::from(buffer0[(i32::from(buffer3[(68_i32) as usize]).wrapping_rem(20_i32)) as usize]) | 81_i32)) as u32;
    buffer2[(18_i32) as usize] = (((i32::from(buffer2[(18_i32) as usize])) as u32).wrapping_sub(((((a.wrapping_mul(a).wrapping_mul(a)) & (((!i32::from(buffer0[(15_i32) as usize]))) as u32))) | (((((i32::from(buffer3[(80_i32) as usize]).wrapping_div(15_i32)) & i32::from(buffer0[(15_i32) as usize])))) as u32)))) as u8;
    buffer3[(88_i32) as usize] = (((i32::from(buffer3[(8_i32) as usize])) as u32).wrapping_add(j).wrapping_add(e).wrapping_sub(((i32::from(buffer0[(i32::from(buffer1[(160_i32) as usize]).wrapping_rem(20_i32)) as usize])) as u32)).wrapping_add((((i32::from(buffer4[(i32::from(buffer0[((((((i32::from(buffer3[(8_i32) as usize])) as u32).wrapping_add(j).wrapping_add(e)) & ((255_i32) as u32))).wrapping_rem(((20_i32) as u32))) as usize]).wrapping_rem(21_i32)) as usize]).wrapping_div(3_i32))) as u32))) as u8;
    b = ((((((r ^ ((i32::from(buffer3[(72_i32) as usize])) as u32))) & (((!198_i32)) as u32))) | (((s.wrapping_mul(s)) & ((198_i32) as u32))))) as u32;
    f = ((((((i32::from(buffer4[(i32::from(buffer1[(69_i32) as usize]).wrapping_rem(21_i32)) as usize]) & i32::from(buffer1[(172_i32) as usize])))) as u32) | ((((((i32::from(buffer4[(i32::from(buffer1[(69_i32) as usize]).wrapping_rem(21_i32)) as usize]) | i32::from(buffer1[(172_i32) as usize])))) as u32) & ((((i32::from(buffer3[(12_i32) as usize])) as u32).wrapping_sub(b)).wrapping_add(((77_i32) as u32))))))) as u32;
    buffer0[(16_i32) as usize] = (((147_i32) as u32).wrapping_sub(((((((i32::from(buffer3[(72_i32) as usize])) as u32) & ((((f & ((251_i32) as u32))) | ((1_i32) as u32))))) | ((((((f & ((250_i32) as u32))) | ((i32::from(buffer3[(72_i32) as usize])) as u32))) & ((198_i32) as u32))))))) as u8;
    c = (((((i32::from(buffer4[(i32::from(buffer1[(168_i32) as usize]).wrapping_rem(21_i32)) as usize]) & i32::from(buffer0[(i32::from(buffer1[(29_i32) as usize]).wrapping_rem(20_i32)) as usize])) & 7_i32)) | ((((i32::from(buffer4[(i32::from(buffer1[(168_i32) as usize]).wrapping_rem(21_i32)) as usize]) | i32::from(buffer0[(i32::from(buffer1[(29_i32) as usize]).wrapping_rem(20_i32)) as usize]))) & 6_i32)))) as u32;
    f = ((((i32::from(buffer4[(i32::from(buffer1[(155_i32) as usize]).wrapping_rem(21_i32)) as usize]) & i32::from(buffer1[(105_i32) as usize]))) | ((((i32::from(buffer4[(i32::from(buffer1[(155_i32) as usize]).wrapping_rem(21_i32)) as usize]) | i32::from(buffer1[(105_i32) as usize]))) & 141_i32)))) as u32;
    buffer0[(3_i32) as usize] = (i32::from(buffer0[(3_i32) as usize]).wrapping_sub(i32::from(buffer4[(weird_rol32((f) as u8, (c) as i32).wrapping_rem(((21_i32) as u32))) as usize]))) as u8;
    buffer1[(5_i32) as usize] = ((weird_ror8((i32::from(buffer0[(12_i32) as usize])) as u8, ((((i32::from(buffer0[(i32::from(buffer1[(61_i32) as usize]).wrapping_rem(20_i32)) as usize]).wrapping_div(5_i32)) & 7_i32))) as i32) ^ (((((((!i32::from(buffer2[(i32::from(buffer3[(84_i32) as usize]).wrapping_rem(35_i32)) as usize])))) as u32) & 0xffffffff_u32)).wrapping_div(((5_i32) as u32))))) as u8;
    buffer1[(198_i32) as usize] = (i32::from(buffer1[(198_i32) as usize]).wrapping_add(i32::from(buffer1[(3_i32) as usize]))) as u8;
    a = (((162_i32 | i32::from(buffer2[(i32::from(buffer3[(64_i32) as usize]).wrapping_rem(35_i32)) as usize])))) as u32;
    buffer1[(164_i32) as usize] = (((i32::from(buffer1[(164_i32) as usize])) as u32).wrapping_add(((a.wrapping_mul(a)).wrapping_div(((5_i32) as u32))))) as u8;
    g = (weird_ror8((139_i32) as u8, (((i32::from(buffer3[(80_i32) as usize]) & 7_i32))) as i32)) as u32;
    c = (((((i32::from(buffer4[(i32::from(buffer3[(64_i32) as usize]).wrapping_rem(21_i32)) as usize]).wrapping_mul(i32::from(buffer4[(i32::from(buffer3[(64_i32) as usize]).wrapping_rem(21_i32)) as usize])).wrapping_mul(i32::from(buffer4[(i32::from(buffer3[(64_i32) as usize]).wrapping_rem(21_i32)) as usize]))) & 95_i32)) | ((i32::from(buffer0[(i32::from(buffer3[(40_i32) as usize]).wrapping_rem(20_i32)) as usize]) & (!95_i32))))) as u32;
    buffer3[(92_i32) as usize] = ((((((g & ((12_i32) as u32))) | ((((i32::from(buffer0[(i32::from(buffer3[(20_i32) as usize]).wrapping_rem(20_i32)) as usize]) & 12_i32))) as u32)) | ((g & ((i32::from(buffer0[(i32::from(buffer3[(20_i32) as usize]).wrapping_rem(20_i32)) as usize])) as u32)))) | c)) as u8;
    buffer2[(12_i32) as usize] = (i32::from(buffer2[(12_i32) as usize]).wrapping_add((((((i32::from(buffer1[(103_i32) as usize]) & 32_i32)) | ((i32::from(buffer3[(92_i32) as usize]) & (((i32::from(buffer1[(103_i32) as usize]) | 60_i32)))))) | 16_i32)).wrapping_div(3_i32))) as u8;
    buffer3[(96_i32) as usize] = (i32::from(buffer1[(143_i32) as usize])) as u8;
    buffer3[(100_i32) as usize] = (27_i32) as u8;
    buffer3[(104_i32) as usize] = ((((((((i32::from(buffer3[(40_i32) as usize]) & (!i32::from(buffer2[(8_i32) as usize])))) | ((i32::from(buffer1[(35_i32) as usize]) & i32::from(buffer2[(8_i32) as usize]))))) & i32::from(buffer3[(64_i32) as usize]))) ^ 119_i32)) as u8;
    buffer3[(108_i32) as usize] = ((238_i32 & (((((((i32::from(buffer3[(40_i32) as usize]) & (!i32::from(buffer2[(8_i32) as usize])))) | ((i32::from(buffer1[(35_i32) as usize]) & i32::from(buffer2[(8_i32) as usize]))))) & i32::from(buffer3[(64_i32) as usize]))).wrapping_shl((1_i32) as u32)))) as u8;
    buffer3[(112_i32) as usize] = (((((!i32::from(buffer3[(64_i32) as usize])) & (i32::from(buffer3[(84_i32) as usize]).wrapping_div(3_i32)))) ^ 49_i32)) as u8;
    buffer3[(116_i32) as usize] = ((98_i32 & ((((!i32::from(buffer3[(64_i32) as usize])) & (i32::from(buffer3[(84_i32) as usize]).wrapping_div(3_i32)))).wrapping_shl((1_i32) as u32)))) as u8;
    a = ((((i32::from(buffer1[(35_i32) as usize]) & i32::from(buffer2[(8_i32) as usize]))) | ((i32::from(buffer3[(40_i32) as usize]) & (!i32::from(buffer2[(8_i32) as usize])))))) as u32;
    b = ((((a & ((i32::from(buffer3[(64_i32) as usize])) as u32))) | ((((((i32::from(buffer3[(84_i32) as usize]).wrapping_div(3_i32)) & (!i32::from(buffer3[(64_i32) as usize])))))) as u32))) as u32;
    buffer1[(143_i32) as usize] = (((i32::from(buffer3[(96_i32) as usize])) as u32).wrapping_sub(((((b & (((86_i32.wrapping_add((((i32::from(buffer1[(172_i32) as usize]) & 64_i32)).wrapping_shr((1_i32) as u32))))) as u32))) | (((((((((((i32::from(buffer1[(172_i32) as usize]) & 65_i32)).wrapping_shr((1_i32) as u32)) ^ 86_i32)) | (((((!i32::from(buffer3[(64_i32) as usize])) & (i32::from(buffer3[(84_i32) as usize]).wrapping_div(3_i32)))) | ((((((i32::from(buffer3[(40_i32) as usize]) & (!i32::from(buffer2[(8_i32) as usize])))) | ((i32::from(buffer1[(35_i32) as usize]) & i32::from(buffer2[(8_i32) as usize]))))) & i32::from(buffer3[(64_i32) as usize]))))))) & i32::from(buffer3[(100_i32) as usize])))) as u32))))) as u8;
    buffer2[(29_i32) as usize] = (162_i32) as u8;
    a = (((((((i32::from(buffer4[(i32::from(buffer3[(88_i32) as usize]).wrapping_rem(21_i32)) as usize])) & 160_i32)) | ((i32::from(buffer0[(i32::from(buffer1[(125_i32) as usize]).wrapping_rem(20_i32)) as usize]) & 95_i32)))).wrapping_shr((1_i32) as u32))) as u32;
    b = ((i32::from(buffer2[(i32::from(buffer1[(149_i32) as usize]).wrapping_rem(35_i32)) as usize]) ^ (i32::from(buffer1[(43_i32) as usize]).wrapping_mul(i32::from(buffer1[(43_i32) as usize]))))) as u32;
    buffer0[(15_i32) as usize] = (((i32::from(buffer0[(15_i32) as usize])) as u32).wrapping_add((((b & a)) | ((((a | b)) & ((115_i32) as u32)))))) as u8;
    buffer3[(120_i32) as usize] = (i32::from(buffer3[(64_i32) as usize]).wrapping_sub(i32::from(buffer0[(i32::from(buffer3[(40_i32) as usize]).wrapping_rem(20_i32)) as usize]))) as u8;
    buffer1[(95_i32) as usize] = (i32::from(buffer4[(i32::from(buffer3[(20_i32) as usize]).wrapping_rem(21_i32)) as usize])) as u8;
    a = (weird_ror8((i32::from(buffer2[(i32::from(buffer3[(80_i32) as usize]).wrapping_rem(35_i32)) as usize])) as u8, (((i32::from(buffer2[(i32::from(buffer1[(17_i32) as usize]).wrapping_rem(35_i32)) as usize]).wrapping_mul(i32::from(buffer2[(i32::from(buffer1[(17_i32) as usize]).wrapping_rem(35_i32)) as usize])).wrapping_mul(i32::from(buffer2[(i32::from(buffer1[(17_i32) as usize]).wrapping_rem(35_i32)) as usize]))) & 7_i32)) as i32)) as u32;
    buffer0[(7_i32) as usize] = (((i32::from(buffer0[(7_i32) as usize])) as u32).wrapping_sub((a.wrapping_mul(a)))) as u8;
    buffer2[(8_i32) as usize] = (i32::from(buffer2[(8_i32) as usize]).wrapping_sub(i32::from(buffer1[(184_i32) as usize])).wrapping_add((i32::from(buffer4[(i32::from(buffer1[(202_i32) as usize]).wrapping_rem(21_i32)) as usize]).wrapping_mul(i32::from(buffer4[(i32::from(buffer1[(202_i32) as usize]).wrapping_rem(21_i32)) as usize])).wrapping_mul(i32::from(buffer4[(i32::from(buffer1[(202_i32) as usize]).wrapping_rem(21_i32)) as usize]))))) as u8;
    buffer0[(16_i32) as usize] = (((i32::from(buffer2[(i32::from(buffer1[(102_i32) as usize]).wrapping_rem(35_i32)) as usize]).wrapping_shl((1_i32) as u32)) & 132_i32)) as u8;
    buffer3[(124_i32) as usize] = (((i32::from(buffer4[(i32::from(buffer3[(40_i32) as usize]).wrapping_rem(21_i32)) as usize]).wrapping_shr((1_i32) as u32)) ^ i32::from(buffer3[(68_i32) as usize]))) as u8;
    buffer0[(7_i32) as usize] = (i32::from(buffer0[(7_i32) as usize]).wrapping_sub((i32::from(buffer0[(i32::from(buffer1[(191_i32) as usize]).wrapping_rem(20_i32)) as usize]).wrapping_sub((((((i32::from(buffer4[(i32::from(buffer1[(80_i32) as usize]).wrapping_rem(21_i32)) as usize]).wrapping_shl((1_i32) as u32)) & (!177_i32))) | ((i32::from(buffer4[(i32::from(buffer4[(i32::from(buffer3[(88_i32) as usize]).wrapping_rem(21_i32)) as usize]).wrapping_rem(21_i32)) as usize]) & 177_i32)))))))) as u8;
    buffer0[(6_i32) as usize] = (i32::from(buffer0[(i32::from(buffer1[(119_i32) as usize]).wrapping_rem(20_i32)) as usize])) as u8;
    a = ((((i32::from(buffer4[(i32::from(buffer1[(190_i32) as usize]).wrapping_rem(21_i32)) as usize]) & (!209_i32))) | ((i32::from(buffer1[(118_i32) as usize]) & 209_i32)))) as u32;
    b = (i32::from(buffer0[(i32::from(buffer3[(120_i32) as usize]).wrapping_rem(20_i32)) as usize]).wrapping_mul(i32::from(buffer0[(i32::from(buffer3[(120_i32) as usize]).wrapping_rem(20_i32)) as usize]))) as u32;
    buffer0[(12_i32) as usize] = ((((((i32::from(buffer0[(i32::from(buffer3[(84_i32) as usize]).wrapping_rem(20_i32)) as usize]) ^ (i32::from(buffer2[(i32::from(buffer1[(71_i32) as usize]).wrapping_rem(35_i32)) as usize]).wrapping_add(i32::from(buffer2[(i32::from(buffer1[(15_i32) as usize]).wrapping_rem(35_i32)) as usize])))))) as u32) & ((((a & b)) | ((((a | b)) & ((27_i32) as u32))))))) as u8;
    b = ((((i32::from(buffer1[(32_i32) as usize]) & i32::from(buffer2[(i32::from(buffer3[(88_i32) as usize]).wrapping_rem(35_i32)) as usize]))) | ((((i32::from(buffer1[(32_i32) as usize]) | i32::from(buffer2[(i32::from(buffer3[(88_i32) as usize]).wrapping_rem(35_i32)) as usize]))) & 23_i32)))) as u32;
    d = ((((((((i32::from(buffer4[(i32::from(buffer1[(57_i32) as usize]).wrapping_rem(21_i32)) as usize]).wrapping_mul(231_i32)) & 169_i32))) as u32) | ((b & ((86_i32) as u32)))))) as u32;
    f = ((((((((i32::from(buffer0[(i32::from(buffer1[(82_i32) as usize]).wrapping_rem(20_i32)) as usize]) & (!29_i32))) | ((i32::from(buffer4[(i32::from(buffer3[(124_i32) as usize]).wrapping_rem(21_i32)) as usize]) & 29_i32)))) & 190_i32)) | ((i32::from(buffer4[((d.wrapping_div(((5_i32) as u32))).wrapping_rem(((21_i32) as u32))) as usize]) & (!190_i32))))) as u32;
    h = (i32::from(buffer0[(i32::from(buffer3[(40_i32) as usize]).wrapping_rem(20_i32)) as usize]).wrapping_mul(i32::from(buffer0[(i32::from(buffer3[(40_i32) as usize]).wrapping_rem(20_i32)) as usize])).wrapping_mul(i32::from(buffer0[(i32::from(buffer3[(40_i32) as usize]).wrapping_rem(20_i32)) as usize]))) as u32;
    k = (((((h & ((i32::from(buffer1[(82_i32) as usize])) as u32))) | ((h & ((92_i32) as u32)))) | ((((i32::from(buffer1[(82_i32) as usize]) & 92_i32))) as u32))) as u32;
    buffer3[(128_i32) as usize] = ((((((f & k)) | ((((f | k)) & ((192_i32) as u32))))) ^ (d.wrapping_div(((5_i32) as u32))))) as u8;
    buffer2[(25_i32) as usize] = ((((i32::from(buffer2[(25_i32) as usize])) as u32) ^ ((((i32::from(buffer0[(i32::from(buffer3[(120_i32) as usize]).wrapping_rem(20_i32)) as usize]).wrapping_shl((1_i32) as u32)).wrapping_mul(i32::from(buffer1[(5_i32) as usize])))) as u32).wrapping_sub(((weird_rol8((i32::from(buffer3[(76_i32) as usize])) as u8, (((i32::from(buffer4[(i32::from(buffer3[(124_i32) as usize]).wrapping_rem(21_i32)) as usize]) & 7_i32))) as i32) & (((i32::from(buffer3[(20_i32) as usize]).wrapping_add(110_i32))) as u32)))))) as u8;
}
