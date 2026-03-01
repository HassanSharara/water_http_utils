mod errors;

use std::collections::HashMap;
pub use errors::*;
use crate::request::CreatingRequestErrors;

/// including all parsed headers
#[derive(Debug)]
pub struct HttpHeaders<'buf, const HL: usize> {
    lines: [HeaderLine<'buf>; HL],
    /// defining content length for public and fast access
    pub content_length: Option<usize>,
    /// defining headers length
    pub headers_length: usize,
}

#[inline(always)]
fn bytes_to_usize(bytes: &[u8]) -> Option<usize> {
    if bytes.is_empty() || bytes.len() > 20 {
        return None;
    }
    let mut result = 0usize;
    for &byte in bytes {
        if byte < b'0' || byte > b'9' {
            return None;
        }
        result = result.checked_mul(10)?.checked_add((byte - b'0') as usize)?;
    }
    Some(result)
}

// ═══════════════════════════════════════════════════════════════════════════════
//  SIMD SCANNER
//  Finds ':' and '\n' in a single pass instead of two separate memchr calls.
//  Three tiers: AVX2 (32B/iter) → SSE2 (16B/iter) → scalar word-at-a-time (8B/iter)
// ═══════════════════════════════════════════════════════════════════════════════

/// Scalar word-at-a-time: processes 8 bytes per iteration on 64-bit targets.
/// Falls back to byte-by-byte only for the tail.
#[inline(always)]
unsafe fn find_colon_and_lf_scalar(slice: &[u8]) -> (usize, usize) {
    const fn splat(b: u8) -> usize {
        usize::from_ne_bytes([b; std::mem::size_of::<usize>()])
    }
    const HI: usize = splat(0x80);
    const LO: usize = splat(0x01);

    #[inline(always)]
    fn has_zero(w: usize) -> bool {
        (w.wrapping_sub(LO) & !w & HI) != 0
    }

    let ptr = slice.as_ptr();
    let len = slice.len();
    let word_size = std::mem::size_of::<usize>();
    let word_count = len / word_size;

    // Word-at-a-time: skip words that contain neither ':' nor '\n'
    let mut wi = 0;
    while wi < word_count {
        let w = (ptr.add(wi * word_size) as *const usize).read_unaligned();
        if has_zero(w ^ splat(b':')) || has_zero(w ^ splat(b'\n')) {
            break;
        }
        wi += 1;
    }

    // Byte scan from the hit word (or start of tail)
    let start = wi * word_size;
    let mut colon = usize::MAX;
    let mut lf    = usize::MAX;
    let mut i = start;
    while i < len {
        let b = *ptr.add(i);
        if b == b':' && colon == usize::MAX { colon = i; }
        if b == b'\n' && lf == usize::MAX   { lf    = i; }
        if colon != usize::MAX && lf != usize::MAX { break; }
        i += 1;
    }
    (colon, lf)
}

/// Find '\n' from a given offset — delegates to memchr (already SIMD-optimised
/// for the single-needle case; no point duplicating it).
#[inline(always)]
unsafe fn find_lf_from(slice: &[u8], from: usize) -> usize {
    memchr::memchr(b'\n', &slice[from..])
        .map(|p| p + from)
        .unwrap_or(usize::MAX)
}

/// AVX2: 32 bytes per cycle, both needles in one pass.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
unsafe fn find_colon_and_lf_avx2(slice: &[u8]) -> (usize, usize) {
    use std::arch::x86_64::*;

    let ptr    = slice.as_ptr();
    let len    = slice.len();
    let vcolon = _mm256_set1_epi8(b':' as i8);
    let vlf    = _mm256_set1_epi8(b'\n' as i8);
    let mut i  = 0usize;

    while i + 32 <= len {
        let chunk  = _mm256_loadu_si256(ptr.add(i) as *const __m256i);
        let mask_c = _mm256_movemask_epi8(_mm256_cmpeq_epi8(chunk, vcolon)) as u32;
        let mask_l = _mm256_movemask_epi8(_mm256_cmpeq_epi8(chunk, vlf))    as u32;

        if mask_c != 0 || mask_l != 0 {
            let pos_c = if mask_c != 0 { i + mask_c.trailing_zeros() as usize } else { usize::MAX };
            let pos_l = if mask_l != 0 { i + mask_l.trailing_zeros() as usize } else { usize::MAX };

            return if pos_c < pos_l {
                // Found ':' first — good. Now find '\n' after it.
                (pos_c, find_lf_from(slice, pos_c + 1))
            } else {
                // '\n' before ':' — malformed header
                (usize::MAX, pos_l)
            };
        }
        i += 32;
    }

    // Scalar tail
    let (c, l) = find_colon_and_lf_scalar(&slice[i..]);
    (
        if c == usize::MAX { usize::MAX } else { c + i },
        if l == usize::MAX { usize::MAX } else { l + i },
    )
}

/// SSE2: 16 bytes per cycle.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
#[inline]
unsafe fn find_colon_and_lf_sse2(slice: &[u8]) -> (usize, usize) {
    use std::arch::x86_64::*;

    let ptr    = slice.as_ptr();
    let len    = slice.len();
    let vcolon = _mm_set1_epi8(b':' as i8);
    let vlf    = _mm_set1_epi8(b'\n' as i8);
    let mut i  = 0usize;

    while i + 16 <= len {
        let chunk  = _mm_loadu_si128(ptr.add(i) as *const __m128i);
        let mask_c = _mm_movemask_epi8(_mm_cmpeq_epi8(chunk, vcolon)) as u32;
        let mask_l = _mm_movemask_epi8(_mm_cmpeq_epi8(chunk, vlf))    as u32;

        if mask_c != 0 || mask_l != 0 {
            let pos_c = if mask_c != 0 { i + mask_c.trailing_zeros() as usize } else { usize::MAX };
            let pos_l = if mask_l != 0 { i + mask_l.trailing_zeros() as usize } else { usize::MAX };

            return if pos_c < pos_l {
                (pos_c, find_lf_from(slice, pos_c + 1))
            } else {
                (usize::MAX, pos_l)
            };
        }
        i += 16;
    }

    let (c, l) = find_colon_and_lf_scalar(&slice[i..]);
    (
        if c == usize::MAX { usize::MAX } else { c + i },
        if l == usize::MAX { usize::MAX } else { l + i },
    )
}

/// Runtime-dispatched entry point — on non-x86 targets goes straight to scalar.
#[inline(always)]
unsafe fn scan_header(slice: &[u8]) -> (usize, usize) {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") {
            return find_colon_and_lf_avx2(slice);
        }
        if is_x86_feature_detected!("sse2") {
            return find_colon_and_lf_sse2(slice);
        }
    }
    find_colon_and_lf_scalar(slice)
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Content-Length detection constants
//  Two overlapping u64 loads cover the full 14-byte key with 2 loads + 2 compares.
//  No loop, no eq_ignore_ascii_case call.
// ═══════════════════════════════════════════════════════════════════════════════
const CONTENT_DASH: u64 = u64::from_le_bytes(*b"content-") | 0x2020_2020_2020_2020;
const LENGTH_WORD:  u64 = u64::from_le_bytes(*b"length\0\0") | 0x2020_2020_2020_2020;
const LENGTH_MASK:  u64 = 0x0000_FFFF_FFFF_FFFF; // only the first 6 bytes

impl<'buf, const HL: usize> HttpHeaders<'buf, HL> {

    #[inline(always)]
    pub(crate) fn new(bytes: &'buf [u8]) -> Result<HttpHeaders<'buf, HL>, CreatingHeadersErrors> {
        let mut lines         = [HeaderLine::empty(); HL];
        let mut lines_index   = 0usize;
        let mut content_length= None::<usize>;

        let base_ptr = bytes.as_ptr();
        let end_ptr  = unsafe { base_ptr.add(bytes.len()) };
        let mut ptr  = base_ptr;

        loop {
            let remaining = unsafe { end_ptr.offset_from(ptr) } as usize;

            if remaining < 2 {
                return Err(CreatingHeadersErrors::ReadMore);
            }

            // ── Terminator: single u16 load = one instruction, no two-byte compare ──
            let first_u16 = unsafe { (ptr as *const u16).read_unaligned() };
            if first_u16 == u16::from_ne_bytes([b'\r', b'\n']) {
                return Ok(HttpHeaders {
                    lines,
                    headers_length: unsafe { ptr.offset_from(base_ptr) } as usize + 2,
                    content_length,
                });
            }

            // ── Single SIMD pass finds ':' AND '\n' simultaneously ────────────────
            let search = unsafe { std::slice::from_raw_parts(ptr, remaining) };
            let (colon_pos, lf_pos) = unsafe { scan_header(search) };

            if colon_pos == usize::MAX || lf_pos == usize::MAX || colon_pos >= lf_pos {
                return Err(CreatingHeadersErrors::ReadMore);
            }

            // ── Zero-copy key and raw value slices ───────────────────────────────
            let key     = unsafe { std::slice::from_raw_parts(ptr, colon_pos) };
            let val_ptr = unsafe { ptr.add(colon_pos + 1) };
            let val_raw = unsafe { std::slice::from_raw_parts(val_ptr, lf_pos - colon_pos - 1) };

            // ── Branchless trim: no if-statements, pure arithmetic → cmov ────────
            let lead = unsafe { (!val_raw.is_empty() && *val_raw.as_ptr() == b' ') as usize };
            let trail = unsafe {
                (!val_raw.is_empty()
                    && *val_raw.as_ptr().add(val_raw.len() - 1) == b'\r') as usize
            };
            let value = unsafe {
                std::slice::from_raw_parts(
                    val_raw.as_ptr().add(lead),
                    val_raw.len().saturating_sub(lead + trail),
                )
            };

            // ── Content-Length: 2 unaligned u64 loads, 2 integer compares ────────
            if key.len() == 14 {
                let w0 = unsafe { (key.as_ptr() as *const u64).read_unaligned() }
                    | 0x2020_2020_2020_2020_u64;
                let w1 = unsafe { (key.as_ptr().add(6) as *const u64).read_unaligned() }
                    | 0x2020_2020_2020_2020_u64;
                if w0 == CONTENT_DASH && (w1 & LENGTH_MASK) == (LENGTH_WORD & LENGTH_MASK) {
                    content_length = bytes_to_usize(value);
                }
            }

            // ── Store (unchecked for zero bounds-check overhead) ─────────────────
            if lines_index < HL {
                unsafe {
                    let line   = lines.get_unchecked_mut(lines_index);
                    line.key   = std::str::from_utf8_unchecked(key);
                    line.value = value.into();
                }
                lines_index += 1;
            }

            // ── Advance past '\n' ────────────────────────────────────────────────
            ptr = unsafe { ptr.add(lf_pos + 1) };
        }
    }

    pub fn get(&self, key: &str) -> Option<&HeaderValue<'buf>> {
        for line in &self.lines {
            if line.key == key {
                return Some(&line.value);
            }
        }
        for line in &self.lines {
            if line.key.to_lowercase() == key.to_lowercase() {
                return Some(&line.value);
            }
        }
        None
    }

    pub fn get_as_str(&self, key: &str) -> Option<&'buf str> {
        self.get(key).map(|v| v.to_str())
    }

    pub fn get_as_bytes(&self, key: &str) -> Option<&'buf [u8]> {
        self.get(key).map(|v| v.bytes)
    }

    pub fn lines(&self) -> Vec<&HeaderLine<'buf>> {
        self.lines.iter().filter(|x| !x.key.is_empty()).collect()
    }
}


impl From<CreatingHeadersErrors> for CreatingRequestErrors {
    fn from(value: CreatingHeadersErrors) -> Self {
        match value {
            CreatingHeadersErrors::InvalidFormat           => CreatingRequestErrors::InvalidHttpFormat,
            CreatingHeadersErrors::MaxHeadersSizeReachedOut => CreatingRequestErrors::DangerousInvalidHttpFormat,
            CreatingHeadersErrors::ReadMore                => CreatingRequestErrors::InsufficientDataSoReadMore,
            CreatingHeadersErrors::DangerousInvalidFormat  => CreatingRequestErrors::DangerousInvalidHttpFormat,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct HeaderLine<'buf> {
    pub key:   &'buf str,
    pub value: HeaderValue<'buf>,
}

impl<'buf> HeaderLine<'buf> {
    pub fn empty() -> HeaderLine<'buf> {
        HeaderLine {
            key:   "",
            value: HeaderValue::new(&[]),
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct HeaderValue<'buf> {
    bytes: &'buf [u8],
}

impl<'buf> HeaderValue<'buf> {
    pub(crate) fn new(bytes: &'buf [u8]) -> HeaderValue<'buf> {
        HeaderValue { bytes }
    }

    pub fn all_injected_values(&self) -> Vec<&'buf str> {
        let v: &'buf str = self.into();
        v.split(", ").collect()
    }

    pub fn all_injected_values_with_params(&self) -> Vec<HeaderVWithParams<'buf>> {
        self.all_injected_values()
            .into_iter()
            .filter_map(|v| HeaderVWithParams::new(v.as_bytes()).ok())
            .collect()
    }

    pub fn to_str(&self) -> &'buf str {
        unsafe { std::str::from_utf8_unchecked(self.bytes) }
    }
}

impl<'buf> Into<HeaderValue<'buf>> for &'buf [u8] {
    fn into(self) -> HeaderValue<'buf> {
        HeaderValue::new(self)
    }
}

impl<'buf> Into<HeaderVWithParams<'buf>> for &'buf str {
    fn into(self) -> HeaderVWithParams<'buf> {
        HeaderVWithParams::new(self.as_bytes()).unwrap()
    }
}

impl<'buf> Into<&'buf str> for &HeaderValue<'buf> {
    fn into(self) -> &'buf str {
        unsafe { std::str::from_utf8_unchecked(self.bytes) }
    }
}

impl Into<String> for HeaderValue<'_> {
    fn into(self) -> String {
        unsafe { std::str::from_utf8_unchecked(self.bytes) }.to_string()
    }
}

#[derive(Debug)]
pub struct HeaderVWithParams<'buf> {
    data:   &'buf [u8],
    value:  &'buf str,
    pub params: HashMap<Option<&'buf str>, &'buf [u8]>,
}

macro_rules! set_value_to_header {
    ($value:ident,$key:ident,$map:ident,$last_index:ident,$index:ident,$bytes:ident) => {
        if let Some(k) = $key {
            $map.insert(
                Some(unsafe { std::str::from_utf8_unchecked(k) }),
                &$bytes[$last_index..$index],
            );
            $key = None;
        } else if $value.is_none() {
            $value = Some(unsafe { std::str::from_utf8_unchecked(&$bytes[$last_index..$index]) });
        } else {
            $map.insert(None, &$bytes[$last_index..$index]);
        }
        $last_index = $index;
    };
}

impl<'buf> HeaderVWithParams<'buf> {
    pub(crate) fn new(bytes: &'buf [u8]) -> Result<HeaderVWithParams<'buf>, ()> {
        let mut map        = HashMap::new();
        let mut value      = None;
        let mut key        = None;
        let mut last_index = 0usize;

        for (index, byte) in bytes.iter().enumerate() {
            match byte {
                &b';' => {
                    set_value_to_header!(value, key, map, last_index, index, bytes);
                }
                &b' ' => {
                    if index == last_index + 1 {
                        if index + 1 >= bytes.len() {
                            return Err(());
                        }
                        last_index = index + 1;
                    }
                }
                &b'=' => {
                    key = Some(&bytes[last_index..index]);
                    if index + 1 >= bytes.len() {
                        return Err(());
                    }
                    last_index = index + 1;
                }
                _ => {}
            }
        }

        if let Some(k) = key {
            map.insert(
                Some(unsafe { std::str::from_utf8_unchecked(k) }),
                &bytes[last_index..],
            );
        }

        Ok(HeaderVWithParams {
            data: bytes,
            value: match value {
                None    => unsafe { std::str::from_utf8_unchecked(bytes) },
                Some(v) => v,
            },
            params: map,
        })
    }

    pub fn to_str(&self) -> &'buf str {
        self.value
    }

    pub fn whole_value_as_str(&self) -> &'buf str {
        unsafe { std::str::from_utf8_unchecked(self.data) }
    }

    pub fn get_param(&self, k: &str) -> Option<&'buf [u8]> {
        if let Some(data) = self.params.get(&Some(k)) {
            return Some(*data);
        }
        for (key, value) in &self.params {
            if key.is_none() && *value == k.as_bytes() {
                return Some(*value);
            }
        }
        None
    }
}