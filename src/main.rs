#![allow(non_camel_case_types)]

use std::fmt;
use std::ptr;
use std::str;
use std::fs::File;
use memmap::Mmap;
use std::path::PathBuf;

use libc::{c_int, c_void, size_t};
use std::net::UdpSocket;

pub struct Regex {
    code: *mut code,
    match_data: *mut match_data,
    ovector: *mut size_t,
}

unsafe impl Send for Regex {}

impl Drop for Regex {
    fn drop(&mut self) {
        unsafe {
            pcre2_match_data_free_8(self.match_data);
            pcre2_code_free_8(self.code);
        }
    }
}

pub struct Error {
    code: c_int,
    offset: size_t,
}

impl Regex {
    pub fn new(pattern: &str) -> Result<Regex, Error> {
        let mut error_code: c_int = 0;
        let mut error_offset: size_t = 0;
        let code = unsafe {
            pcre2_compile_8(
                pattern.as_ptr(),
                pattern.len(),
                // PCRE2 can get significantly faster in some cases depending
                // on the permutation of these options (in particular, dropping
                // UCP). We should endeavor to have a separate "ASCII compatible"
                // benchmark.
                PCRE2_UCP | PCRE2_UTF,
                &mut error_code,
                &mut error_offset,
                ptr::null_mut(),
            )
        };
        if code.is_null() {
            return Err(Error { code: error_code, offset: error_offset });
        }
        let err = unsafe { pcre2_jit_compile_8(code, PCRE2_JIT_COMPLETE) };
        if err < 0 {
            panic!("pcre2_jit_compile_8 failed with error: {:?}", err);
        }
        let match_data = unsafe {
            pcre2_match_data_create_from_pattern_8(code, ptr::null_mut())
        };
        if match_data.is_null() {
            panic!("could not allocate match_data");
        }
        let ovector = unsafe { pcre2_get_ovector_pointer_8(match_data) };
        if ovector.is_null() {
            panic!("could not get ovector");
        }
        Ok(Regex { code: code, match_data: match_data, ovector: ovector })
    }

    pub fn is_match(&self, text: &str) -> bool {
        self.find_at(text, 0).is_some()
    }

    pub fn find_iter<'r, 't>(&'r self, text: &'t str) -> FindMatches<'r, 't> {
        FindMatches { re: self, text: text, last_match_end: 0 }
    }

    fn find_at(&self, text: &str, start: usize) -> Option<(usize, usize)> {
        let err = unsafe {
            pcre2_jit_match_8(
                self.code,
                text.as_ptr(),
                text.len(),
                start,
                PCRE2_NO_UTF_CHECK,
                self.match_data,
                ptr::null_mut(),
            )
        };
        if err == PCRE2_ERROR_NOMATCH {
            None
        } else if err < 0 {
            panic!("unknown error code: {:?}", err)
        } else {
            Some(unsafe { (*self.ovector, *self.ovector.offset(1)) })
        }
    }
}

pub struct FindMatches<'r, 't> {
    re: &'r Regex,
    text: &'t str,
    last_match_end: usize,
}

impl<'r, 't> Iterator for FindMatches<'r, 't> {
    type Item = (usize, usize);

    fn next(&mut self) -> Option<(usize, usize)> {
        match self.re.find_at(self.text, self.last_match_end) {
            None => None,
            Some((s, e)) => {
                self.last_match_end = e;
                Some((s, e))
            }
        }
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        const BUF_LEN: size_t = 256;
        let mut buf = [0; BUF_LEN];
        let len = unsafe {
            pcre2_get_error_message_8(self.code, buf.as_mut_ptr(), BUF_LEN)
        };
        if len < 0 {
            write!(
                f,
                "Unknown PCRE error. (code: {:?}, offset: {:?})",
                self.code, self.offset
            )
        } else {
            let msg = str::from_utf8(&buf[..len as usize]).unwrap();
            write!(f, "error at {:?}: {}", self.offset, msg)
        }
    }
}

// PCRE2 FFI. We only wrap the bits we need.

const PCRE2_UCP: u32 = 0x00020000;
const PCRE2_UTF: u32 = 0x00080000;
const PCRE2_NO_UTF_CHECK: u32 = 0x40000000;
const PCRE2_JIT_COMPLETE: u32 = 0x00000001;
const PCRE2_ERROR_NOMATCH: c_int = -1;

type code = c_void;

type match_data = c_void;

type compile_context = c_void; // unused

type general_context = c_void; // unused

type match_context = c_void; // unused

#[link(name = "pcre2-8")]
extern "C" {
    fn pcre2_compile_8(
        pattern: *const u8,
        len: size_t,
        options: u32,
        error_code: *mut c_int,
        error_offset: *mut size_t,
        context: *mut compile_context,
    ) -> *mut code;

    fn pcre2_code_free_8(code: *mut code);

    fn pcre2_match_data_create_from_pattern_8(
        code: *const code,
        context: *mut general_context,
    ) -> *mut match_data;

    fn pcre2_match_data_free_8(match_data: *mut match_data);

    fn pcre2_get_ovector_pointer_8(match_data: *mut match_data)
        -> *mut size_t;

    fn pcre2_jit_compile_8(code: *const code, options: u32) -> c_int;

    fn pcre2_jit_match_8(
        code: *const code,
        subject: *const u8,
        length: size_t,
        startoffset: size_t,
        options: u32,
        match_data: *mut match_data,
        match_context: *mut match_context,
    ) -> c_int;

    fn pcre2_get_error_message_8(
        error_code: c_int,
        buf: *mut u8,
        buflen: size_t,
    ) -> c_int;
}

fn main () {
    let mut refind = String::from("");
    let pat = "\\d{4}[^\\d\\s]{3,11}\\S{1}";//1. \\d{4}: match 4 digital number   2. [^\\d\\s]: no digtal number,no blank character 
                                            //3. {3,11} lenth rang frong 3 to 10  4. \\S{1} match no blank character to it's tail
    let mut config_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));//get project absolute pash
    config_path.push("input");//push input file
    let mmap =
    unsafe { Mmap::map(&File::open(config_path).unwrap()).unwrap() };//opfen this file and unwarp it
    let haystack = unsafe { str::from_utf8_unchecked(&mmap) };//Converts input bytes to a string slice without checking that the string contains valid UTF-8
    let hs = Regex::new(pat).unwrap();
    let hs = hs.find_iter(haystack).into_iter();

    for one in hs {
        print!("find one match:  {:?},   {:?}\n", &haystack[one.0..one.1], &haystack[one.0+4..one.1-1]);
        refind.push_str(&haystack[one.0+4..one.1-1]);
        refind.push_str(" ".into());
    }
    
    let refind = refind.trim();
    let socket = UdpSocket::bind("127.0.0.1:3829").expect("Bind error!");
    loop {
        let mut buf = [0u8; 1500];
        let (_amt, src) = socket.recv_from(&mut buf).expect("Recive error!");

        println!(
            "recv: {}",
            std::str::from_utf8(&buf).expect("Print error!")
        );
        socket.send_to(refind.as_bytes(), &src).expect("send error!");
    }
}