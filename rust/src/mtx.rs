//! C ABI replacements for Dolphin/libultra matrix and vector helpers.
//!
//! This module preserves the row-major PC matrix routines and the packed
//! fixed-point representation consumed by the existing C runtime.

use std::ffi::c_void;

const DEG_TO_RAD: f32 = std::f32::consts::PI / 180.0;
const HILITE_THRESHOLD: f32 = 0.1;

#[repr(C)]
pub struct PCVec {
    x: f32,
    y: f32,
    z: f32,
}

unsafe fn get3(matrix: *const f32, row: usize, col: usize) -> f32 {
    // SAFETY: Callers provide a readable 3x4 matrix.
    unsafe { *matrix.add(row * 4 + col) }
}

unsafe fn set3(matrix: *mut f32, row: usize, col: usize, value: f32) {
    // SAFETY: Callers provide a writable 3x4 matrix.
    unsafe { *matrix.add(row * 4 + col) = value };
}

unsafe fn set4(matrix: *mut f32, row: usize, col: usize, value: f32) {
    // SAFETY: Callers provide a writable 4x4 matrix.
    unsafe { *matrix.add(row * 4 + col) = value };
}

unsafe fn identity3(matrix: *mut f32) {
    for row in 0..3 {
        for col in 0..4 {
            // SAFETY: The output is a 3x4 matrix.
            unsafe { set3(matrix, row, col, if row == col { 1.0 } else { 0.0 }) };
        }
    }
}

unsafe fn identity4(matrix: *mut f32) {
    for row in 0..4 {
        for col in 0..4 {
            // SAFETY: The output is a 4x4 matrix.
            unsafe { set4(matrix, row, col, if row == col { 1.0 } else { 0.0 }) };
        }
    }
}

unsafe fn mtx_f2l(matrix: *const f32, packed: *mut c_void) {
    let words = packed.cast::<u32>();
    for row in 0..4 {
        for pair in 0..2 {
            // C's long is 32-bit in the required i686 builds. Cast truncates
            // the fixed-point values to the same low 32 bits before packing.
            let first = (unsafe { *matrix.add(row * 4 + pair * 2) } * 65536.0) as i32;
            let second = (unsafe { *matrix.add(row * 4 + pair * 2 + 1) } * 65536.0) as i32;
            let index = row * 2 + pair;
            let integer_word = (first as u32 & 0xffff_0000) | ((second >> 16) as u32 & 0xffff);
            let fraction_word = ((first as u32) << 16 & 0xffff_0000) | (second as u32 & 0xffff);
            // SAFETY: A libultra Mtx contains 16 words. The first 8 words are
            // the integer halves and the next 8 are the fractional halves.
            unsafe {
                words.add(index).write(integer_word);
                words.add(index + 8).write(fraction_word);
            }
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn PSMTXIdentity(matrix: *mut [f32; 4]) {
    // SAFETY: The C API supplies three writable rows.
    unsafe { identity3(matrix.cast()) };
}

#[no_mangle]
pub unsafe extern "C" fn C_MTXIdentity(matrix: *mut [f32; 4]) {
    // SAFETY: This is the same 3x4 identity operation as PSMTXIdentity.
    unsafe { PSMTXIdentity(matrix) };
}

#[no_mangle]
pub unsafe extern "C" fn PSMTXCopy(source: *const [f32; 4], destination: *mut [f32; 4]) {
    // SAFETY: The C API supplies 12 readable and writable floats; copy permits overlap.
    unsafe { std::ptr::copy(source.cast::<f32>(), destination.cast::<f32>(), 12) };
}

#[no_mangle]
pub unsafe extern "C" fn PSMTXConcat(
    a: *const [f32; 4],
    b: *const [f32; 4],
    result: *mut [f32; 4],
) {
    let (a, b, result) = (a.cast::<f32>(), b.cast::<f32>(), result.cast::<f32>());
    let mut output = [0.0f32; 12];
    for row in 0..3 {
        for col in 0..3 {
            for inner in 0..3 {
                // SAFETY: Inputs are readable 3x4 matrices.
                output[row * 4 + col] += unsafe { get3(a, row, inner) * get3(b, inner, col) };
            }
        }
        // SAFETY: Inputs are readable 3x4 matrices.
        output[row * 4 + 3] = unsafe {
            get3(a, row, 0) * get3(b, 0, 3)
                + get3(a, row, 1) * get3(b, 1, 3)
                + get3(a, row, 2) * get3(b, 2, 3)
                + get3(a, row, 3)
        };
    }
    // SAFETY: The C API supplies 12 writable result floats.
    unsafe { std::ptr::copy_nonoverlapping(output.as_ptr(), result, 12) };
}

#[no_mangle]
pub unsafe extern "C" fn PSMTXInverse(source: *const [f32; 4], inverse: *mut [f32; 4]) {
    let source = source.cast::<f32>();
    // SAFETY: The C API supplies a readable 3x4 source matrix.
    let s = |row, col| unsafe { get3(source, row, col) };
    let determinant = s(0, 0) * (s(1, 1) * s(2, 2) - s(1, 2) * s(2, 1))
        - s(0, 1) * (s(1, 0) * s(2, 2) - s(1, 2) * s(2, 0))
        + s(0, 2) * (s(1, 0) * s(2, 1) - s(1, 1) * s(2, 0));
    if determinant.abs() < 1e-25 {
        // SAFETY: The C API supplies three writable rows.
        unsafe { identity3(inverse.cast()) };
        return;
    }

    let reciprocal = 1.0 / determinant;
    let mut output = [0.0f32; 12];
    output[0] = (s(1, 1) * s(2, 2) - s(1, 2) * s(2, 1)) * reciprocal;
    output[1] = (s(0, 2) * s(2, 1) - s(0, 1) * s(2, 2)) * reciprocal;
    output[2] = (s(0, 1) * s(1, 2) - s(0, 2) * s(1, 1)) * reciprocal;
    output[4] = (s(1, 2) * s(2, 0) - s(1, 0) * s(2, 2)) * reciprocal;
    output[5] = (s(0, 0) * s(2, 2) - s(0, 2) * s(2, 0)) * reciprocal;
    output[6] = (s(0, 2) * s(1, 0) - s(0, 0) * s(1, 2)) * reciprocal;
    output[8] = (s(1, 0) * s(2, 1) - s(1, 1) * s(2, 0)) * reciprocal;
    output[9] = (s(0, 1) * s(2, 0) - s(0, 0) * s(2, 1)) * reciprocal;
    output[10] = (s(0, 0) * s(1, 1) - s(0, 1) * s(1, 0)) * reciprocal;
    for row in 0..3 {
        output[row * 4 + 3] = -(output[row * 4] * s(0, 3)
            + output[row * 4 + 1] * s(1, 3)
            + output[row * 4 + 2] * s(2, 3));
    }
    // SAFETY: The C API supplies 12 writable destination floats.
    unsafe { std::ptr::copy_nonoverlapping(output.as_ptr(), inverse.cast(), 12) };
}

#[no_mangle]
pub unsafe extern "C" fn PSMTXMultVec(
    matrix: *const [f32; 4],
    source: *const PCVec,
    destination: *mut PCVec,
) {
    let matrix = matrix.cast::<f32>();
    // SAFETY: The C API supplies one readable vector and a readable 3x4 matrix.
    let (x, y, z) = unsafe { ((*source).x, (*source).y, (*source).z) };
    let result = unsafe {
        PCVec {
            x: get3(matrix, 0, 0) * x
                + get3(matrix, 0, 1) * y
                + get3(matrix, 0, 2) * z
                + get3(matrix, 0, 3),
            y: get3(matrix, 1, 0) * x
                + get3(matrix, 1, 1) * y
                + get3(matrix, 1, 2) * z
                + get3(matrix, 1, 3),
            z: get3(matrix, 2, 0) * x
                + get3(matrix, 2, 1) * y
                + get3(matrix, 2, 2) * z
                + get3(matrix, 2, 3),
        }
    };
    // SAFETY: Destination points to one writable vector.
    unsafe { destination.write(result) };
}

#[no_mangle]
pub unsafe extern "C" fn PSMTXMultVecSR(
    matrix: *const [f32; 4],
    source: *const PCVec,
    destination: *mut PCVec,
) {
    let matrix = matrix.cast::<f32>();
    // SAFETY: The C API supplies one readable vector and a readable 3x4 matrix.
    let (x, y, z) = unsafe { ((*source).x, (*source).y, (*source).z) };
    let result = unsafe {
        PCVec {
            x: get3(matrix, 0, 0) * x + get3(matrix, 0, 1) * y + get3(matrix, 0, 2) * z,
            y: get3(matrix, 1, 0) * x + get3(matrix, 1, 1) * y + get3(matrix, 1, 2) * z,
            z: get3(matrix, 2, 0) * x + get3(matrix, 2, 1) * y + get3(matrix, 2, 2) * z,
        }
    };
    // SAFETY: Destination points to one writable vector.
    unsafe { destination.write(result) };
}

#[no_mangle]
pub unsafe extern "C" fn PSMTXMultVecArray(
    matrix: *const [f32; 4],
    source: *const PCVec,
    destination: *mut PCVec,
    count: u32,
) {
    for index in 0..count as usize {
        // SAFETY: The C API supplies arrays with at least `count` elements.
        unsafe { PSMTXMultVec(matrix, source.add(index), destination.add(index)) };
    }
}

#[no_mangle]
pub unsafe extern "C" fn PSMTXScale(matrix: *mut [f32; 4], sx: f32, sy: f32, sz: f32) {
    let values = [sx, 0.0, 0.0, 0.0, 0.0, sy, 0.0, 0.0, 0.0, 0.0, sz, 0.0];
    // SAFETY: The C API supplies 12 writable floats.
    unsafe { std::ptr::copy_nonoverlapping(values.as_ptr(), matrix.cast(), 12) };
}

#[no_mangle]
pub unsafe extern "C" fn PSMTXTrans(matrix: *mut [f32; 4], tx: f32, ty: f32, tz: f32) {
    let values = [1.0, 0.0, 0.0, tx, 0.0, 1.0, 0.0, ty, 0.0, 0.0, 1.0, tz];
    // SAFETY: The C API supplies 12 writable floats.
    unsafe { std::ptr::copy_nonoverlapping(values.as_ptr(), matrix.cast(), 12) };
}

#[no_mangle]
pub unsafe extern "C" fn PSMTXTransApply(
    source: *const [f32; 4],
    destination: *mut [f32; 4],
    tx: f32,
    ty: f32,
    tz: f32,
) {
    if !std::ptr::eq(source, destination.cast_const()) {
        // SAFETY: The C API supplies readable and writable 3x4 matrices.
        unsafe { PSMTXCopy(source, destination) };
    }
    let destination = destination.cast::<f32>();
    // SAFETY: The C API supplies 12 writable floats.
    unsafe {
        *destination.add(3) += tx;
        *destination.add(7) += ty;
        *destination.add(11) += tz;
    }
}

#[no_mangle]
pub unsafe extern "C" fn PSMTXScaleApply(
    source: *const [f32; 4],
    destination: *mut [f32; 4],
    sx: f32,
    sy: f32,
    sz: f32,
) {
    let source = source.cast::<f32>();
    let destination = destination.cast::<f32>();
    for col in 0..4 {
        // SAFETY: The C API supplies 12 readable and writable floats.
        unsafe {
            *destination.add(col) = *source.add(col) * sx;
            *destination.add(4 + col) = *source.add(4 + col) * sy;
            *destination.add(8 + col) = *source.add(8 + col) * sz;
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn PSVECNormalize(source: *const PCVec, destination: *mut PCVec) {
    // SAFETY: The C API supplies one readable vector.
    let source = unsafe { &*source };
    let magnitude = (source.x * source.x + source.y * source.y + source.z * source.z).sqrt();
    let result = if magnitude > 0.0 {
        let inverse = 1.0 / magnitude;
        PCVec {
            x: source.x * inverse,
            y: source.y * inverse,
            z: source.z * inverse,
        }
    } else {
        PCVec {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        }
    };
    // SAFETY: Destination points to one writable vector.
    unsafe { destination.write(result) };
}

#[no_mangle]
pub unsafe extern "C" fn PSVECCrossProduct(
    a: *const PCVec,
    b: *const PCVec,
    destination: *mut PCVec,
) {
    // SAFETY: The C API supplies two readable vectors.
    let (a, b) = unsafe { (&*a, &*b) };
    let result = PCVec {
        x: a.y * b.z - a.z * b.y,
        y: a.z * b.x - a.x * b.z,
        z: a.x * b.y - a.y * b.x,
    };
    // SAFETY: Destination points to one writable vector.
    unsafe { destination.write(result) };
}

#[no_mangle]
pub unsafe extern "C" fn PSVECDotProduct(a: *const PCVec, b: *const PCVec) -> f32 {
    // SAFETY: The C API supplies two readable vectors.
    let (a, b) = unsafe { (&*a, &*b) };
    a.x * b.x + a.y * b.y + a.z * b.z
}

#[no_mangle]
pub unsafe extern "C" fn PSVECMag(vector: *const PCVec) -> f32 {
    // SAFETY: The C API supplies one readable vector.
    let vector = unsafe { &*vector };
    (vector.x * vector.x + vector.y * vector.y + vector.z * vector.z).sqrt()
}

#[no_mangle]
pub unsafe extern "C" fn C_MTXFrustum(
    matrix: *mut [f32; 4],
    t: f32,
    b: f32,
    l: f32,
    r: f32,
    n: f32,
    f: f32,
) {
    let m = matrix.cast::<f32>();
    let x = 1.0 / (r - l);
    let y = 1.0 / (t - b);
    let z = 1.0 / (f - n);
    let values = [
        2.0 * n * x,
        0.0,
        (r + l) * x,
        0.0,
        0.0,
        2.0 * n * y,
        (t + b) * y,
        0.0,
        0.0,
        0.0,
        -n * z,
        -(f * n) * z,
        0.0,
        0.0,
        -1.0,
        0.0,
    ];
    // SAFETY: The C API supplies 16 writable floats.
    unsafe { std::ptr::copy_nonoverlapping(values.as_ptr(), m, 16) };
}

#[no_mangle]
pub unsafe extern "C" fn C_MTXPerspective(
    matrix: *mut [f32; 4],
    fov_y: f32,
    aspect: f32,
    n: f32,
    f: f32,
) {
    let angle = 0.5 * fov_y * DEG_TO_RAD;
    let cotangent = 1.0 / angle.tan();
    let reciprocal = 1.0 / (f - n);
    let values = [
        cotangent / aspect,
        0.0,
        0.0,
        0.0,
        0.0,
        cotangent,
        0.0,
        0.0,
        0.0,
        0.0,
        -n * reciprocal,
        -(f * n) * reciprocal,
        0.0,
        0.0,
        -1.0,
        0.0,
    ];
    // SAFETY: The C API supplies 16 writable floats.
    unsafe { std::ptr::copy_nonoverlapping(values.as_ptr(), matrix.cast(), 16) };
}

#[no_mangle]
pub unsafe extern "C" fn C_MTXOrtho(
    matrix: *mut [f32; 4],
    t: f32,
    b: f32,
    l: f32,
    r: f32,
    n: f32,
    f: f32,
) {
    let x = 1.0 / (r - l);
    let y = 1.0 / (t - b);
    let z = 1.0 / (f - n);
    let values = [
        2.0 * x,
        0.0,
        0.0,
        -(r + l) * x,
        0.0,
        2.0 * y,
        0.0,
        -(t + b) * y,
        0.0,
        0.0,
        -z,
        -n * z,
        0.0,
        0.0,
        0.0,
        1.0,
    ];
    // SAFETY: The C API supplies 16 writable floats.
    unsafe { std::ptr::copy_nonoverlapping(values.as_ptr(), matrix.cast(), 16) };
}

#[no_mangle]
pub unsafe extern "C" fn C_MTXLookAt(
    matrix: *mut [f32; 4],
    camera: *const PCVec,
    camera_up: *const PCVec,
    target: *const PCVec,
) {
    // SAFETY: The C API supplies three readable vectors.
    let (camera, camera_up, target) = unsafe { (&*camera, &*camera_up, &*target) };
    let mut look = PCVec {
        x: camera.x - target.x,
        y: camera.y - target.y,
        z: camera.z - target.z,
    };
    normalize(&mut look);
    let mut right = cross(camera_up, &look);
    normalize(&mut right);
    let up = cross(&look, &right);
    let values = [
        right.x,
        right.y,
        right.z,
        -(camera.x * right.x + camera.y * right.y + camera.z * right.z),
        up.x,
        up.y,
        up.z,
        -(camera.x * up.x + camera.y * up.y + camera.z * up.z),
        look.x,
        look.y,
        look.z,
        -(camera.x * look.x + camera.y * look.y + camera.z * look.z),
    ];
    // SAFETY: The C API supplies a writable 3x4 matrix.
    unsafe { std::ptr::copy_nonoverlapping(values.as_ptr(), matrix.cast(), 12) };
}

fn cross(a: &PCVec, b: &PCVec) -> PCVec {
    PCVec {
        x: a.y * b.z - a.z * b.y,
        y: a.z * b.x - a.x * b.z,
        z: a.x * b.y - a.y * b.x,
    }
}

fn normalize(vector: &mut PCVec) {
    let magnitude = (vector.x * vector.x + vector.y * vector.y + vector.z * vector.z).sqrt();
    if magnitude > 0.0 {
        vector.x /= magnitude;
        vector.y /= magnitude;
        vector.z /= magnitude;
    } else {
        vector.x = 0.0;
        vector.y = 0.0;
        vector.z = 0.0;
    }
}

fn normalize_unchecked(vector: &mut PCVec, sign: f32) {
    let magnitude = (vector.x * vector.x + vector.y * vector.y + vector.z * vector.z).sqrt();
    let inverse = sign / magnitude;
    vector.x *= inverse;
    vector.y *= inverse;
    vector.z *= inverse;
}

#[no_mangle]
pub unsafe extern "C" fn C_MTXLightPerspective(
    matrix: *mut [f32; 4],
    fov_y: f32,
    aspect: f32,
    scale_s: f32,
    scale_t: f32,
    trans_s: f32,
    trans_t: f32,
) {
    let cotangent = 1.0 / (0.5 * fov_y * DEG_TO_RAD).tan();
    let values = [
        cotangent / aspect * scale_s,
        0.0,
        -trans_s,
        0.0,
        0.0,
        cotangent * scale_t,
        -trans_t,
        0.0,
        0.0,
        0.0,
        -1.0,
        0.0,
    ];
    // SAFETY: The C API supplies a writable 3x4 matrix.
    unsafe { std::ptr::copy_nonoverlapping(values.as_ptr(), matrix.cast(), 12) };
}

#[no_mangle]
pub unsafe extern "C" fn C_MTXLightOrtho(
    matrix: *mut [f32; 4],
    t: f32,
    b: f32,
    l: f32,
    r: f32,
    scale_s: f32,
    scale_t: f32,
    trans_s: f32,
    trans_t: f32,
) {
    let x = 1.0 / (r - l);
    let y = 1.0 / (t - b);
    let values = [
        2.0 * x * scale_s,
        0.0,
        0.0,
        (-(r + l) * x) * scale_s + trans_s,
        0.0,
        2.0 * y * scale_t,
        0.0,
        (-(t + b) * y) * scale_t + trans_t,
        0.0,
        0.0,
        0.0,
        1.0,
    ];
    // SAFETY: The C API supplies a writable 3x4 matrix.
    unsafe { std::ptr::copy_nonoverlapping(values.as_ptr(), matrix.cast(), 12) };
}

#[no_mangle]
pub unsafe extern "C" fn guMtxIdentF(matrix: *mut [f32; 4]) {
    // SAFETY: The C API supplies four writable rows.
    unsafe { identity4(matrix.cast()) };
}

#[no_mangle]
pub unsafe extern "C" fn guMtxF2L(matrix: *const [f32; 4], packed: *mut c_void) {
    // SAFETY: The C API supplies 16 readable floats and a writable packed Mtx.
    unsafe { mtx_f2l(matrix.cast(), packed) };
}

#[no_mangle]
pub unsafe extern "C" fn guMtxIdent(packed: *mut c_void) {
    let mut matrix = [[0.0f32; 4]; 4];
    // SAFETY: Local matrix and packed output have the required sizes.
    unsafe {
        guMtxIdentF(matrix.as_mut_ptr());
        guMtxF2L(matrix.as_ptr(), packed);
    }
}

#[no_mangle]
pub unsafe extern "C" fn guOrthoF(
    matrix: *mut [f32; 4],
    l: f32,
    r: f32,
    b: f32,
    t: f32,
    n: f32,
    f: f32,
    scale: f32,
) {
    let mut values = [[0.0f32; 4]; 4];
    // SAFETY: Local matrix has the required size.
    unsafe { guMtxIdentF(values.as_mut_ptr()) };
    values[0][0] = 2.0 / (r - l);
    values[1][1] = 2.0 / (t - b);
    values[2][2] = -2.0 / (f - n);
    values[3][0] = -(r + l) / (r - l);
    values[3][1] = -(t + b) / (t - b);
    values[3][2] = -(f + n) / (f - n);
    values[3][3] = 1.0;
    for row in &mut values {
        for value in row {
            *value *= scale;
        }
    }
    // SAFETY: The C API supplies 16 writable floats.
    unsafe { std::ptr::copy_nonoverlapping(values.as_ptr().cast::<f32>(), matrix.cast(), 16) };
}

#[no_mangle]
pub unsafe extern "C" fn guOrtho(
    packed: *mut c_void,
    l: f32,
    r: f32,
    b: f32,
    t: f32,
    n: f32,
    f: f32,
    scale: f32,
) {
    let mut matrix = [[0.0f32; 4]; 4];
    // SAFETY: Local matrix and packed output have the required sizes.
    unsafe {
        guOrthoF(matrix.as_mut_ptr(), l, r, b, t, n, f, scale);
        guMtxF2L(matrix.as_ptr(), packed);
    }
}

#[no_mangle]
pub unsafe extern "C" fn guPerspectiveF(
    matrix: *mut [f32; 4],
    norm: *mut u16,
    fovy: f32,
    aspect: f32,
    near: f32,
    far: f32,
    scale: f32,
) {
    let mut values = [[0.0f32; 4]; 4];
    // SAFETY: Local matrix has the required size.
    unsafe { guMtxIdentF(values.as_mut_ptr()) };
    let fovy = fovy * DEG_TO_RAD;
    let cotangent = (fovy / 2.0).cos() / (fovy / 2.0).sin();
    values[0][0] = cotangent / aspect;
    values[1][1] = cotangent;
    values[2][2] = (near + far) / (near - far);
    values[2][3] = -1.0;
    values[3][2] = (2.0 * near * far) / (near - far);
    values[3][3] = 0.0;
    for row in &mut values {
        for value in row {
            *value *= scale;
        }
    }
    if !norm.is_null() {
        // SAFETY: A non-null norm pointer is writable per the C API contract.
        unsafe {
            *norm = if near + far <= 2.0 {
                0xffff
            } else {
                let value = ((2.0 * 65536.0) / (near + far)) as u16;
                if value == 0 {
                    1
                } else {
                    value
                }
            };
        }
    }
    // SAFETY: The C API supplies 16 writable floats.
    unsafe { std::ptr::copy_nonoverlapping(values.as_ptr().cast::<f32>(), matrix.cast(), 16) };
}

#[no_mangle]
pub unsafe extern "C" fn guPerspective(
    packed: *mut c_void,
    norm: *mut u16,
    fovy: f32,
    aspect: f32,
    near: f32,
    far: f32,
    scale: f32,
) {
    let mut matrix = [[0.0f32; 4]; 4];
    // SAFETY: Local matrix and packed output have the required sizes.
    unsafe {
        guPerspectiveF(matrix.as_mut_ptr(), norm, fovy, aspect, near, far, scale);
        guMtxF2L(matrix.as_ptr(), packed);
    }
}

#[no_mangle]
pub unsafe extern "C" fn guLookAtF(
    matrix: *mut [f32; 4],
    x_eye: f32,
    y_eye: f32,
    z_eye: f32,
    x_at: f32,
    y_at: f32,
    z_at: f32,
    x_up: f32,
    y_up: f32,
    z_up: f32,
) {
    let mut look = PCVec {
        x: x_at - x_eye,
        y: y_at - y_eye,
        z: z_at - z_eye,
    };
    // libultra uses -1 / length directly; keep its zero-length IEEE behavior.
    normalize_unchecked(&mut look, -1.0);
    let up_seed = PCVec {
        x: x_up,
        y: y_up,
        z: z_up,
    };
    let mut right = cross(&up_seed, &look);
    normalize_unchecked(&mut right, 1.0);
    let mut up = cross(&look, &right);
    normalize_unchecked(&mut up, 1.0);
    let values = [
        right.x,
        up.x,
        look.x,
        0.0,
        right.y,
        up.y,
        look.y,
        0.0,
        right.z,
        up.z,
        look.z,
        0.0,
        -(x_eye * right.x + y_eye * right.y + z_eye * right.z),
        -(x_eye * up.x + y_eye * up.y + z_eye * up.z),
        -(x_eye * look.x + y_eye * look.y + z_eye * look.z),
        1.0,
    ];
    // SAFETY: The C API supplies 16 writable floats.
    unsafe { std::ptr::copy_nonoverlapping(values.as_ptr(), matrix.cast(), 16) };
}

#[no_mangle]
pub unsafe extern "C" fn guLookAt(
    packed: *mut c_void,
    x_eye: f32,
    y_eye: f32,
    z_eye: f32,
    x_at: f32,
    y_at: f32,
    z_at: f32,
    x_up: f32,
    y_up: f32,
    z_up: f32,
) {
    let mut matrix = [[0.0f32; 4]; 4];
    // SAFETY: Local matrix and packed output have the required sizes.
    unsafe {
        guLookAtF(
            matrix.as_mut_ptr(),
            x_eye,
            y_eye,
            z_eye,
            x_at,
            y_at,
            z_at,
            x_up,
            y_up,
            z_up,
        );
        guMtxF2L(matrix.as_ptr(), packed);
    }
}

#[no_mangle]
pub unsafe extern "C" fn guLookAtHilite(
    packed: *mut c_void,
    look_at: *mut c_void,
    hilite: *mut c_void,
    x_eye: f32,
    y_eye: f32,
    z_eye: f32,
    x_at: f32,
    y_at: f32,
    z_at: f32,
    x_up: f32,
    y_up: f32,
    z_up: f32,
    x_light1: f32,
    y_light1: f32,
    z_light1: f32,
    x_light2: f32,
    y_light2: f32,
    z_light2: f32,
    width: i32,
    height: i32,
) {
    let mut look = PCVec {
        x: x_at - x_eye,
        y: y_at - y_eye,
        z: z_at - z_eye,
    };
    normalize_unchecked(&mut look, -1.0);
    let mut right = PCVec {
        x: y_up * look.z - z_up * look.y,
        y: z_up * look.x - x_up * look.z,
        z: x_up * look.y - y_up * look.x,
    };
    normalize_unchecked(&mut right, 1.0);
    let mut up = cross(&look, &right);
    normalize_unchecked(&mut up, 1.0);

    let mut hilite_values = [0i32; 4];
    for (offset, (lx, ly, lz)) in [
        (0usize, (x_light1, y_light1, z_light1)),
        (2, (x_light2, y_light2, z_light2)),
    ] {
        let length = (lx * lx + ly * ly + lz * lz).sqrt();
        let inverse = 1.0 / length;
        let (lx, ly, lz) = (lx * inverse, ly * inverse, lz * inverse);
        let mut h = PCVec {
            x: lx + look.x,
            y: ly + look.y,
            z: lz + look.z,
        };
        let magnitude = (h.x * h.x + h.y * h.y + h.z * h.z).sqrt();
        if magnitude > HILITE_THRESHOLD {
            let inverse = 1.0 / magnitude;
            h.x *= inverse;
            h.y *= inverse;
            h.z *= inverse;
            hilite_values[offset] = width * 4
                + ((h.x * right.x + h.y * right.y + h.z * right.z) * width as f32 * 2.0) as i32;
            hilite_values[offset + 1] =
                height * 4 + ((h.x * up.x + h.y * up.y + h.z * up.z) * height as f32 * 2.0) as i32;
        } else {
            hilite_values[offset] = width * 2;
            hilite_values[offset + 1] = height * 2;
        }
    }
    // SAFETY: The C API supplies the packed hilite and look-at structures.
    unsafe {
        std::ptr::copy_nonoverlapping(hilite_values.as_ptr(), hilite.cast::<i32>(), 4);
        let lights = look_at.cast::<u8>();
        let directions = [right.x, right.y, right.z, up.x, up.y, up.z];
        for (index, value) in directions.into_iter().enumerate() {
            let scaled = (value * 128.0).min(127.0) as i32 & 0xff;
            lights
                .add(if index < 3 { 8 + index } else { 24 + index - 3 })
                .write(scaled as u8);
        }
        lights.add(0).write(0);
        lights.add(1).write(0);
        lights.add(2).write(0);
        lights.add(4).write(0);
        lights.add(5).write(0);
        lights.add(6).write(0);
        lights.add(16).write(0);
        lights.add(17).write(0x80);
        lights.add(18).write(0);
        lights.add(20).write(0);
        lights.add(21).write(0x80);
        lights.add(22).write(0);
    }

    let matrix = [
        right.x,
        up.x,
        look.x,
        0.0,
        right.y,
        up.y,
        look.y,
        0.0,
        right.z,
        up.z,
        look.z,
        0.0,
        -(x_eye * right.x + y_eye * right.y + z_eye * right.z),
        -(x_eye * up.x + y_eye * up.y + z_eye * up.z),
        -(x_eye * look.x + y_eye * look.y + z_eye * look.z),
        1.0,
    ];
    // SAFETY: Packed output is a libultra Mtx.
    unsafe { mtx_f2l(matrix.as_ptr(), packed) };
}

#[no_mangle]
pub unsafe extern "C" fn guScale(packed: *mut c_void, x: f32, y: f32, z: f32) {
    let mut matrix = [[0.0f32; 4]; 4];
    // SAFETY: Local matrix and packed output have the required sizes.
    unsafe {
        guMtxIdentF(matrix.as_mut_ptr());
        matrix[0][0] = x;
        matrix[1][1] = y;
        matrix[2][2] = z;
        guMtxF2L(matrix.as_ptr(), packed);
    }
}

#[no_mangle]
pub unsafe extern "C" fn guTranslate(packed: *mut c_void, x: f32, y: f32, z: f32) {
    let mut matrix = [[0.0f32; 4]; 4];
    // SAFETY: Local matrix and packed output have the required sizes.
    unsafe {
        guMtxIdentF(matrix.as_mut_ptr());
        matrix[3][0] = x;
        matrix[3][1] = y;
        matrix[3][2] = z;
        guMtxF2L(matrix.as_ptr(), packed);
    }
}

#[no_mangle]
pub unsafe extern "C" fn guRotateF(
    matrix: *mut [f32; 4],
    mut angle: f32,
    mut x: f32,
    mut y: f32,
    mut z: f32,
) {
    let length = (x * x + y * y + z * z).sqrt();
    if length > 0.0 {
        x /= length;
        y /= length;
        z /= length;
    }
    angle *= DEG_TO_RAD;
    let (sine, cosine) = angle.sin_cos();
    let t = 1.0 - cosine;
    let mut values = [[0.0f32; 4]; 4];
    // SAFETY: Local matrix has the required size.
    unsafe { guMtxIdentF(values.as_mut_ptr()) };
    values[0][0] = t * x * x + cosine;
    values[0][1] = t * x * y + sine * z;
    values[0][2] = t * x * z - sine * y;
    values[1][0] = t * x * y - sine * z;
    values[1][1] = t * y * y + cosine;
    values[1][2] = t * y * z + sine * x;
    values[2][0] = t * x * z + sine * y;
    values[2][1] = t * y * z - sine * x;
    values[2][2] = t * z * z + cosine;
    // SAFETY: The C API supplies 16 writable floats.
    unsafe { std::ptr::copy_nonoverlapping(values.as_ptr().cast::<f32>(), matrix.cast(), 16) };
}

#[no_mangle]
pub unsafe extern "C" fn guRotate(packed: *mut c_void, angle: f32, x: f32, y: f32, z: f32) {
    let mut matrix = [[0.0f32; 4]; 4];
    // SAFETY: Local matrix and packed output have the required sizes.
    unsafe {
        guRotateF(matrix.as_mut_ptr(), angle, x, y, z);
        guMtxF2L(matrix.as_ptr(), packed);
    }
}

#[no_mangle]
pub unsafe extern "C" fn guNormalize(x: *mut f32, y: *mut f32, z: *mut f32) {
    // SAFETY: The C API supplies three writable floats.
    let (mut x_value, mut y_value, mut z_value) = unsafe { (*x, *y, *z) };
    let norm = (x_value * x_value + y_value * y_value + z_value * z_value).sqrt();
    if norm > 0.0 {
        let inverse = 1.0 / norm;
        x_value *= inverse;
        y_value *= inverse;
        z_value *= inverse;
        // SAFETY: The C API supplies three writable floats.
        unsafe {
            *x = x_value;
            *y = y_value;
            *z = z_value
        };
    }
}
