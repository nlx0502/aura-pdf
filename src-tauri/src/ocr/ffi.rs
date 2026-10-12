//! API C de Tesseract (capi.h) : seules les fonctions utilisées sont déclarées.
#![allow(non_snake_case, dead_code)]

use std::os::raw::{c_char, c_float, c_int, c_uchar};

#[repr(C)]
pub struct TessBaseAPI {
    _p: [u8; 0],
}
#[repr(C)]
pub struct TessResultIterator {
    _p: [u8; 0],
}
#[repr(C)]
pub struct TessPageIterator {
    _p: [u8; 0],
}
#[repr(C)]
pub struct EtextDesc {
    _p: [u8; 0],
}

pub const RIL_BLOCK: c_int = 0;
pub const RIL_PARA: c_int = 1;
pub const RIL_TEXTLINE: c_int = 2;
pub const RIL_WORD: c_int = 3;

extern "C" {
    pub fn TessBaseAPICreate() -> *mut TessBaseAPI;
    pub fn TessBaseAPIDelete(api: *mut TessBaseAPI);
    pub fn TessBaseAPIEnd(api: *mut TessBaseAPI);
    pub fn TessBaseAPIInit3(api: *mut TessBaseAPI, datapath: *const c_char, language: *const c_char) -> c_int;
    pub fn TessBaseAPISetPageSegMode(api: *mut TessBaseAPI, mode: c_int);
    pub fn TessBaseAPISetImage(api: *mut TessBaseAPI, imagedata: *const c_uchar, width: c_int, height: c_int, bytes_per_pixel: c_int, bytes_per_line: c_int);
    pub fn TessBaseAPISetSourceResolution(api: *mut TessBaseAPI, ppi: c_int);
    pub fn TessBaseAPIRecognize(api: *mut TessBaseAPI, monitor: *mut EtextDesc) -> c_int;
    pub fn TessBaseAPIClear(api: *mut TessBaseAPI);
    pub fn TessBaseAPIGetIterator(api: *mut TessBaseAPI) -> *mut TessResultIterator;
    pub fn TessResultIteratorDelete(it: *mut TessResultIterator);
    pub fn TessResultIteratorGetPageIterator(it: *mut TessResultIterator) -> *mut TessPageIterator;
    pub fn TessResultIteratorGetUTF8Text(it: *const TessResultIterator, level: c_int) -> *mut c_char;
    pub fn TessResultIteratorConfidence(it: *const TessResultIterator, level: c_int) -> c_float;
    pub fn TessPageIteratorNext(it: *mut TessPageIterator, level: c_int) -> c_int;
    pub fn TessPageIteratorIsAtBeginningOf(it: *const TessPageIterator, level: c_int) -> c_int;
    pub fn TessPageIteratorBoundingBox(it: *const TessPageIterator, level: c_int, left: *mut c_int, top: *mut c_int, right: *mut c_int, bottom: *mut c_int) -> c_int;
    pub fn TessPageIteratorOrientation(it: *mut TessPageIterator, orientation: *mut c_int, writing_direction: *mut c_int, textline_order: *mut c_int, deskew_angle: *mut c_float);
    pub fn TessDeleteText(text: *const c_char);
    pub fn TessVersion() -> *const c_char;
}
