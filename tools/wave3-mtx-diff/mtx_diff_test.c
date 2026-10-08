/*
 * mtx_diff_test.c -- Wave 3 differential bit-pattern test for the mtx.rs port.
 *
 * Question under test (rewiring.md, Wave 3): is Rust's f32::sin_cos()
 * bit-identical to separate sinf()/cosf() calls on Philip's MSVC/MinGW
 * toolchain? The only mtx.rs function affected is guRotateF (rust/src/mtx.rs).
 * PSMTXConcat and guRotateF op order were already verified line-by-line;
 * sin_cos() vs sinf()/cosf() is the one open bit question.
 *
 * How it works:
 *   1. c_guRotateF below is copied VERBATIM from src/pc_mtx.c (only renamed),
 *      so it uses the toolchain's own sinf()/cosf().
 *   2. guRotateF is the REAL Rust export, compiled from rust/src/mtx.rs with
 *      the same rustc that builds the game (see run_mtx_diff.sh).
 *   3. The driver feeds both a grid of angles x axes and compares every one
 *      of the 16 output floats as raw bit patterns (u32), not as floats.
 *
 * Build/run on Philip's Windows machine (MSYS2 bash, from the repo root):
 *      bash tools/wave3-mtx-diff/run_mtx_diff.sh
 *
 * Self-test of this driver (no Rust needed, proves the harness itself works):
 *      gcc -DSELFTEST -O2 tools/wave3-mtx-diff/mtx_diff_test.c -o /tmp/mtx_selftest -lm
 *      /tmp/mtx_selftest        # must print ALL TESTS PASSED
 */
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#define PC_PIf 3.14159265358979323846f
#define PC_DEG_TO_RADf (PC_PIf / 180.0f)

/* ------------------------------------------------------------------ */
/* Reference implementation: verbatim copy of src/pc_mtx.c, renamed.    */
/* guMtxIdentF:                                                        */
static void c_guMtxIdentF(float mf[4][4]) {
    int i, j;
    for (i = 0; i < 4; i++)
        for (j = 0; j < 4; j++)
            mf[i][j] = (i == j) ? 1.0f : 0.0f;
}

/* guRotateF, src/pc_mtx.c lines 541-556: */
static void c_guRotateF(float mf[4][4], float a, float x, float y, float z) {
    float s, c, t;
    float len = sqrtf(x*x + y*y + z*z);
    if (len > 0.0f) { x /= len; y /= len; z /= len; }

    a *= PC_DEG_TO_RADf;
    s = sinf(a);
    c = cosf(a);
    t = 1.0f - c;

    c_guMtxIdentF(mf);
    mf[0][0] = t*x*x + c;    mf[0][1] = t*x*y + s*z; mf[0][2] = t*x*z - s*y;
    mf[1][0] = t*x*y - s*z;  mf[1][1] = t*y*y + c;   mf[1][2] = t*y*z + s*x;
    mf[2][0] = t*x*z + s*y;  mf[2][1] = t*y*z - s*x; mf[2][2] = t*z*z + c;
}
/* ------------------------------------------------------------------ */

#ifdef SELFTEST
/* Driver self-check: Rust side replaced by a second copy of the C formula. */
void guRotateF(float mf[4][4], float a, float x, float y, float z) {
    c_guRotateF(mf, a, x, y, z);
}
#else
/* The real Rust export, from rust/src/mtx.rs compiled with rustc. */
extern void guRotateF(float mf[4][4], float a, float x, float y, float z);
#endif

static uint32_t fbits(float f) {
    uint32_t u;
    memcpy(&u, &f, sizeof u);
    return u;
}

int main(void) {
    /* Angle grid: exact landmarks, fractions, negatives, multi-turn, tiny. */
    static const float angles[] = {
        0.0f, 1.0f, 7.3f, 30.0f, 45.0f, 89.9f, 90.0f, 90.1f, 135.0f,
        179.9f, 180.0f, 180.1f, 270.0f, 359.9f, 360.0f, 360.1f,
        -45.0f, -90.0f, -180.0f, 720.5f, 1080.25f, 0.001f, 123.456f,
        33.333f, 66.667f, 210.0f, 315.0f
    };
    /* Axis grid: cardinal, diagonal, unnormalized, zero, odd. */
    static const float axes[][3] = {
        {1,0,0}, {0,1,0}, {0,0,1}, {1,1,1}, {1,1,0}, {1,0,1}, {0,1,1},
        {-1,0,0}, {0,-1,0}, {0,0,-1}, {3,4,0}, {1,2,3}, {0,0,0},
        {0.5f,0.5f,0.7071f}, {100,1,1}, {1,100,1}
    };

    long total = 0, mismatched_cases = 0, mismatched_floats = 0;
    for (unsigned ai = 0; ai < sizeof angles / sizeof angles[0]; ai++) {
        for (unsigned xi = 0; xi < sizeof axes / sizeof axes[0]; xi++) {
            float mc[4][4], mr[4][4];
            c_guRotateF(mc, angles[ai], axes[xi][0], axes[xi][1], axes[xi][2]);
            guRotateF(mr, angles[ai], axes[xi][0], axes[xi][1], axes[xi][2]);
            total++;
            int case_bad = 0;
            for (int r = 0; r < 4; r++) {
                for (int c = 0; c < 4; c++) {
                    uint32_t bc = fbits(mc[r][c]), br = fbits(mr[r][c]);
                    if (bc != br) {
                        mismatched_floats++;
                        if (!case_bad) {
                            printf("MISMATCH angle=%.4f axis=(%.4f,%.4f,%.4f)\n",
                                   angles[ai], axes[xi][0], axes[xi][1], axes[xi][2]);
                            case_bad = 1;
                            mismatched_cases++;
                        }
                        printf("  [%d][%d] C=0x%08X Rust=0x%08X (C=%a Rust=%a)\n",
                               r, c, bc, br, mc[r][c], mr[r][c]);
                    }
                }
            }
        }
    }
    printf("----\n");
    printf("cases: %ld, mismatched cases: %ld, mismatched floats: %ld\n",
           total, mismatched_cases, mismatched_floats);
    if (mismatched_floats == 0) {
        printf("ALL TESTS PASSED: guRotateF is bit-identical (sin_cos == sinf+cosf here).\n");
        return 0;
    }
    printf("FAILED: bit differences found; do NOT enable the pc_mtx.c exclusion yet.\n");
    return 1;
}
