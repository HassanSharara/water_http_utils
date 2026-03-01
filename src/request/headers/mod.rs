mod errors;

pub use errors::*;
use core::mem::MaybeUninit;

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

/// including all parsed headers
#[derive(Debug)]
pub struct HttpHeaders<'buf, const HL: usize> {
    pub lines: [HeaderLine<'buf>; HL],
    pub content_length: Option<usize>,
    pub headers_length: usize,
    pub count: usize,
}

#[inline(always)]
fn bytes_to_usize(bytes: &[u8]) -> Option<usize> {
    if bytes.is_empty() || bytes.len() > 20 { return None; }
    let mut result = 0usize;
    for &byte in bytes {
        if byte < b'0' || byte > b'9' { return None; }
        result = result.checked_mul(10)?.checked_add((byte - b'0') as usize)?;
    }
    Some(result)
}

impl<'buf, const HL: usize> HttpHeaders<'buf, HL> {
    #[inline(always)]
    pub(crate) fn new(bytes: &'buf [u8]) -> Result<HttpHeaders<'buf, HL>, CreatingHeadersErrors> {
        let mut lines: [HeaderLine<'buf>; HL] = unsafe { MaybeUninit::uninit().assume_init() };
        let mut lines_index = 0_usize;
        let mut content_length = None;

        let base_ptr = bytes.as_ptr();
        let end_ptr = unsafe { base_ptr.add(bytes.len()) };
        let mut ptr = base_ptr;

        unsafe {
            while ptr < end_ptr {
                let remaining = end_ptr.offset_from(ptr) as usize;

                // 1. End of Headers Check (\r\n)
                if remaining >= 2 {
                    if (ptr as *const u16).read_unaligned() == 0x0A0D {
                        return Ok(HttpHeaders {
                            lines,
                            headers_length: ptr.offset_from(base_ptr) as usize + 2,
                            count: lines_index,
                            content_length,
                        });
                    }
                } else { return Err(CreatingHeadersErrors::ReadMore); }

                // 2. SIMD Search for ':'
                let mut c_pos = None;
                #[cfg(target_arch = "x86_64")]
                if remaining >= 16 {
                    let chunk = _mm_loadu_si128(ptr as *const __m128i);
                    let mask = _mm_movemask_epi8(_mm_cmpeq_epi8(chunk, _mm_set1_epi8(b':' as i8))) as u32;
                    if mask != 0 { c_pos = Some(mask.trailing_zeros() as usize); }
                }

                let colon_idx = match c_pos {
                    Some(i) => i,
                    None => match memchr::memchr(b':', std::slice::from_raw_parts(ptr, remaining)) {
                        Some(i) => i,
                        None => return Err(CreatingHeadersErrors::ReadMore),
                    }
                };

                let key_raw = std::slice::from_raw_parts(ptr, colon_idx);
                let mut val_ptr = ptr.add(colon_idx + 1);
                if val_ptr < end_ptr && *val_ptr == b' ' { val_ptr = val_ptr.add(1); }

                // 3. SIMD Search for '\n'
                let val_rem = end_ptr.offset_from(val_ptr) as usize;
                let mut l_pos = None;
                #[cfg(target_arch = "x86_64")]
                if val_rem >= 16 {
                    let chunk = _mm_loadu_si128(val_ptr as *const __m128i);
                    let mask = _mm_movemask_epi8(_mm_cmpeq_epi8(chunk, _mm_set1_epi8(b'\n' as i8))) as u32;
                    if mask != 0 { l_pos = Some(mask.trailing_zeros() as usize); }
                }

                let lf_idx = match l_pos {
                    Some(i) => i,
                    None => match memchr::memchr(b'\n', std::slice::from_raw_parts(val_ptr, val_rem)) {
                        Some(i) => i,
                        None => return Err(CreatingHeadersErrors::ReadMore),
                    }
                };

                let mut v_len = lf_idx;
                if v_len > 0 && *val_ptr.add(v_len - 1) == b'\r' { v_len -= 1; }
                let val_raw = std::slice::from_raw_parts(val_ptr, v_len);

                // 4. Content-Length Optimization
                if key_raw.len() == 14 && (key_raw[0] | 32) == b'c' {
                    if key_raw.eq_ignore_ascii_case(b"content-length") {
                        content_length = bytes_to_usize(val_raw);
                    }
                }

                // 5. Safe write to uninitialized memory
                if lines_index < HL {
                    std::ptr::write(lines.as_mut_ptr().add(lines_index), HeaderLine {
                        key: std::str::from_utf8_unchecked(key_raw),
                        value: HeaderValue::new(val_raw),
                    });
                    lines_index += 1;
                } else { return Err(CreatingHeadersErrors::MaxHeadersSizeReachedOut); }

                ptr = val_ptr.add(lf_idx + 1);
            }
        }
        Err(CreatingHeadersErrors::ReadMore)
    }

    pub fn get(&self, key: &str) -> Option<&HeaderValue<'buf>> {
        for i in 0..self.count {
            let line = unsafe { self.lines.get_unchecked(i) };
            if line.key.eq_ignore_ascii_case(key) {
                return Some(&line.value);
            }
        }
        None
    }

    pub fn get_as_str(&self, key: &str) -> Option<&'buf str> {
        self.get(key).map(|v| v.to_str())
    }

    pub fn lines(&self) -> Vec<&HeaderLine<'buf>> {
        self.lines.iter().take(self.count).collect()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct HeaderLine<'buf> {
    pub key: &'buf str,
    pub value: HeaderValue<'buf>
}

#[derive(Debug, Copy, Clone)]
pub struct HeaderValue<'buf> {
    pub bytes: &'buf [u8],
}

impl<'buf> HeaderValue<'buf> {
    pub fn new(bytes: &'buf [u8]) -> Self { Self { bytes } }
    pub fn to_str(&self) -> &'buf str { unsafe { std::str::from_utf8_unchecked(self.bytes) } }
}

// Fixed conversions
impl<'buf> From<&'buf [u8]> for HeaderValue<'buf> {
    fn from(bytes: &'buf [u8]) -> Self { HeaderValue::new(bytes) }
}

impl<'a, 'buf> From<&'a HeaderValue<'buf>> for &'buf str {
    fn from(value: &'a HeaderValue<'buf>) -> Self { value.to_str() }
}