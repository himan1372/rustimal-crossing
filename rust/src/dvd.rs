//! PC implementation of the DVD filesystem compatibility layer.
//!
//! The exported Dolphin DVD functions retain their C ABI. DVDFileInfo remains
//! owned by the C caller; this module uses the same 0x18/0x30/0x34 fields as
//! the previous PC implementation and delegates disc reads to the Rust disc
//! reader in the parent crate.

use std::ffi::{c_char, c_int, c_long, c_void, CStr, CString};
use std::ptr;
use std::sync::Mutex;

const MAX_DVD_ENTRIES: usize = 512;
const DVD_FILE_INFO_SIZE: usize = 0x3c;
const FILE_POINTER_OFFSET: usize = 0x18;
const START_ADDRESS_OFFSET: usize = 0x30;
const LENGTH_OFFSET: usize = 0x34;
const DISC_SENTINEL: usize = 0xDEAD_C0DE;

#[repr(C)]
struct DvdDiskId {
    game_name: [u8; 4],
    company: [u8; 2],
    disk_number: u8,
    game_version: u8,
    streaming: u8,
    stream_buffer_size: u8,
    padding: [u8; 22],
}

static mut DISK_ID: DvdDiskId = DvdDiskId {
    game_name: *b"GAFE",
    company: *b"01",
    disk_number: 0,
    game_version: 0,
    streaming: 0,
    stream_buffer_size: 0,
    padding: [0; 22],
};

static DVD_ENTRIES: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());
static ASSETS_BASE_PATH: Mutex<Option<Vec<u8>>> = Mutex::new(None);

extern "C" {
    fn fopen(path: *const c_char, mode: *const c_char) -> *mut c_void;
    fn fclose(stream: *mut c_void) -> c_int;
    fn fseek(stream: *mut c_void, offset: c_long, origin: c_int) -> c_int;
    fn ftell(stream: *mut c_void) -> c_long;
    fn fread(buffer: *mut c_void, size: usize, count: usize, stream: *mut c_void) -> usize;
}

fn file_info_field<T>(file_info: *mut c_void, offset: usize) -> *mut T {
    // The C ABI contract supplies a writable, aligned DVDFileInfo (0x3c bytes).
    unsafe { file_info.cast::<u8>().add(offset).cast::<T>() }
}

fn c_path(path: &[u8]) -> Option<CString> {
    CString::new(path).ok()
}

unsafe fn open_file(path: &[u8]) -> *mut c_void {
    let Some(path) = c_path(path) else {
        return ptr::null_mut();
    };
    // SAFETY: Both strings are NUL terminated and remain live through fopen.
    unsafe { fopen(path.as_ptr(), b"rb\0".as_ptr().cast()) }
}

fn fallback_base_path() -> Vec<u8> {
    let mut base_path = ASSETS_BASE_PATH.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(path) = base_path.as_ref() {
        return path.clone();
    }

    let candidates: [&[u8]; 6] = [
        b"assets/files",
        b"assets",
        b"../assets/files",
        b"../assets",
        b"../../assets/files",
        b"../../assets",
    ];
    let mut selected = b"assets".to_vec();
    for candidate in candidates {
        let mut marker = candidate.to_vec();
        marker.extend_from_slice(b"/COPYDATE");
        // SAFETY: `marker` contains no NUL bytes and is converted by open_file.
        let file = unsafe { open_file(&marker) };
        if !file.is_null() {
            // SAFETY: `file` is a successful fopen result.
            unsafe { fclose(file) };
            selected = candidate.to_vec();
            break;
        }
    }
    *base_path = Some(selected.clone());
    selected
}

fn fallback_file_path(base: &[u8], path: &[u8]) -> Vec<u8> {
    let mut full_path = base.to_vec();
    if !path.first().is_some_and(|byte| *byte == b'/') {
        full_path.push(b'/');
    }
    full_path.extend_from_slice(path);
    full_path
}

#[no_mangle]
pub extern "C" fn DVDGetCurrentDiskID() -> *mut c_void {
    // SAFETY: This pointer refers to the stable process-wide 0x20-byte disk ID.
    unsafe { ptr::addr_of_mut!(DISK_ID).cast() }
}

#[no_mangle]
pub unsafe extern "C" fn DVDConvertPathToEntrynum(path: *const c_char) -> i32 {
    if path.is_null() {
        return -1;
    }
    // SAFETY: The Dolphin DVD ABI supplies a NUL-terminated path.
    let path = unsafe { CStr::from_ptr(path) }.to_bytes();
    let mut entries = DVD_ENTRIES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(index) = entries.iter().position(|entry| entry.as_slice() == path) {
        return index as i32;
    }
    if entries.len() >= MAX_DVD_ENTRIES {
        eprintln!("[PC/DVD] Entry table full ({MAX_DVD_ENTRIES} entries)! Cannot register path");
        return -1;
    }

    let mut entry = vec![0; 256];
    let copy_len = path.len().min(entry.len() - 1);
    entry[..copy_len].copy_from_slice(&path[..copy_len]);
    entry.truncate(copy_len);
    let index = entries.len();
    entries.push(entry);
    index as i32
}

#[no_mangle]
pub unsafe extern "C" fn DVDFastOpen(entry_num: i32, file_info: *mut c_void) -> i32 {
    if file_info.is_null() {
        return 0;
    }
    let path = {
        let entries = DVD_ENTRIES.lock().unwrap_or_else(|e| e.into_inner());
        let Some(path) = usize::try_from(entry_num).ok().and_then(|i| entries.get(i)) else {
            return 0;
        };
        path.clone()
    };

    if super::pc_disc_is_open() != 0 {
        let Some(path) = c_path(&path) else { return 0 };
        let (mut disc_offset, mut file_size) = (0u32, 0u32);
        // SAFETY: The C string and output pointers are valid for this call.
        if unsafe {
            super::pc_disc_find_file(path.as_ptr(), &mut disc_offset, &mut file_size)
        } != 0
        {
            // SAFETY: The caller supplies the complete writable DVDFileInfo.
            unsafe {
                ptr::write_bytes(file_info.cast::<u8>(), 0, DVD_FILE_INFO_SIZE);
                file_info_field::<*mut c_void>(file_info, FILE_POINTER_OFFSET)
                    .write(DISC_SENTINEL as *mut c_void);
                file_info_field::<u32>(file_info, START_ADDRESS_OFFSET).write(disc_offset);
                file_info_field::<u32>(file_info, LENGTH_OFFSET).write(file_size);
            }
            return 1;
        }
    }

    let base_path = fallback_base_path();
    let full_path = fallback_file_path(&base_path, &path);
    // SAFETY: The path is NUL-checked by open_file before passing it to fopen.
    let file = unsafe { open_file(&full_path) };
    if file.is_null() {
        return 0;
    }
    // SAFETY: `file` is open; the previous C shim also used 32-bit fseek/ftell.
    let length = unsafe {
        fseek(file, 0, 2);
        let length = ftell(file) as u32;
        fseek(file, 0, 0);
        length
    };
    // SAFETY: The caller supplies the complete writable DVDFileInfo.
    unsafe {
        ptr::write_bytes(file_info.cast::<u8>(), 0, DVD_FILE_INFO_SIZE);
        file_info_field::<*mut c_void>(file_info, FILE_POINTER_OFFSET).write(file);
        file_info_field::<u32>(file_info, START_ADDRESS_OFFSET).write(0);
        file_info_field::<u32>(file_info, LENGTH_OFFSET).write(length);
    }
    1
}

#[no_mangle]
pub unsafe extern "C" fn DVDOpen(filename: *const c_char, file_info: *mut c_void) -> i32 {
    if filename.is_null() {
        return 0;
    }
    // SAFETY: The Dolphin DVD ABI supplies a NUL-terminated path.
    let entry = unsafe { DVDConvertPathToEntrynum(filename) };
    if entry < 0 {
        return 0;
    }
    // SAFETY: The caller provides the DVDFileInfo storage.
    unsafe { DVDFastOpen(entry, file_info) }
}

#[no_mangle]
pub unsafe extern "C" fn DVDClose(file_info: *mut c_void) -> i32 {
    if file_info.is_null() {
        return 0;
    }
    // SAFETY: The C ABI supplies an initialized DVDFileInfo.
    let file = unsafe { file_info_field::<*mut c_void>(file_info, FILE_POINTER_OFFSET).read() };
    if !file.is_null() && file as usize != DISC_SENTINEL {
        // SAFETY: Non-sentinel values were returned by fopen in DVDFastOpen.
        unsafe { fclose(file) };
    }
    // SAFETY: The field lies within the writable DVDFileInfo.
    unsafe { file_info_field::<*mut c_void>(file_info, FILE_POINTER_OFFSET).write(ptr::null_mut()) };
    1
}

#[no_mangle]
pub unsafe extern "C" fn DVDReadPrio(
    file_info: *mut c_void,
    buffer: *mut c_void,
    length: i32,
    offset: i32,
    _priority: i32,
) -> i32 {
    if file_info.is_null() || length < 0 || (buffer.is_null() && length != 0) {
        return -1;
    }
    // SAFETY: The caller supplies an initialized DVDFileInfo.
    let file = unsafe { file_info_field::<*mut c_void>(file_info, FILE_POINTER_OFFSET).read() };
    if file as usize == DISC_SENTINEL {
        // SAFETY: Both file info fields are within the caller's DVDFileInfo.
        let base = unsafe { file_info_field::<u32>(file_info, START_ADDRESS_OFFSET).read() };
        if unsafe { super::pc_disc_read(base.wrapping_add(offset as u32), buffer, length as u32) }
            != 0
        {
            return length;
        }
        return -1;
    }
    if file.is_null() {
        return -1;
    }
    // SAFETY: `file` is an open FILE* and the buffer has at least `length` writable bytes.
    unsafe {
        fseek(file, offset as c_long, 0);
        fread(buffer, 1, length as usize, file) as i32
    }
}

#[no_mangle]
pub unsafe extern "C" fn DVDRead(
    file_info: *mut c_void,
    buffer: *mut c_void,
    length: i32,
    offset: i32,
) -> i32 {
    // SAFETY: The caller's pointers and lengths follow the DVD read ABI.
    unsafe { DVDReadPrio(file_info, buffer, length, offset, 2) }
}

#[no_mangle]
pub unsafe extern "C" fn DVDGetLength(file_info: *mut c_void) -> u32 {
    if file_info.is_null() {
        return 0;
    }
    // SAFETY: The length field lies within a valid DVDFileInfo.
    unsafe { file_info_field::<u32>(file_info, LENGTH_OFFSET).read() }
}

type DvdCallback = unsafe extern "C" fn(i32, *mut c_void);

#[no_mangle]
pub unsafe extern "C" fn DVDReadAsyncPrio(
    file_info: *mut c_void,
    buffer: *mut c_void,
    length: i32,
    offset: i32,
    callback: Option<DvdCallback>,
    priority: i32,
) -> i32 {
    // SAFETY: Arguments are forwarded unchanged to the synchronous implementation.
    let read = unsafe { DVDReadPrio(file_info, buffer, length, offset, priority) };
    if let Some(callback) = callback {
        // SAFETY: The callback follows the Dolphin DVDCallback C ABI.
        unsafe { callback(read, file_info) };
    }
    1
}

#[no_mangle]
pub extern "C" fn OSDVDFatalError() {
    eprintln!("[PC/DVD] Fatal DVD error");
}

#[no_mangle]
pub extern "C" fn DVDInit() {}

#[no_mangle]
pub extern "C" fn DVDSetAutoFatalMessaging(_enable: i32) {}

#[no_mangle]
pub extern "C" fn DVDGetFileInfoStatus(_file_info: *mut c_void) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn DVDGetTransferredSize(_file_info: *mut c_void) -> i32 {
    0
}

#[no_mangle]
pub unsafe extern "C" fn DVDFastClose(file_info: *mut c_void) -> i32 {
    // SAFETY: This is an alias of DVDClose with the same ABI.
    unsafe { DVDClose(file_info) }
}

#[no_mangle]
pub extern "C" fn DVDGetDriveStatus() -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn DVDCancel(_block: *mut c_void) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn DVDCancelAsync(_block: *mut c_void, _callback: *mut c_void) -> i32 {
    1
}

#[no_mangle]
pub extern "C" fn DVDChangeDisk(_block: *mut c_void, _disk_id: *mut c_void) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn DVDChangeDiskAsync(
    _block: *mut c_void,
    _disk_id: *mut c_void,
    _callback: *mut c_void,
) -> i32 {
    1
}

#[no_mangle]
pub extern "C" fn DVDGetCommandBlockStatus(_block: *mut c_void) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn DVDPrepareStreamAsync(
    _file_info: *mut c_void,
    _length: u32,
    _offset: u32,
    _callback: *mut c_void,
) -> i32 {
    1
}

#[no_mangle]
pub extern "C" fn DVDCancelStream(_block: *mut c_void) -> i32 {
    0
}
