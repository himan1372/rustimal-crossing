/* pc_gba_joybus.c - Virtual GBA link: Dolphin-compatible TCP JOYBUS master.
 *
 * Replaces the dead GBA stubs that used to live in pc_stubs.c. Implements the
 * GameCube side of the JOYBUS protocol over TCP, speaking Dolphin's GBA server
 * protocol, so an off-the-shelf GBA emulator can act as the link peer:
 *   - mGBA:  File -> Connect to Dolphin (0.9.0+)
 *   - VBA-M: Options -> Link -> Joybus options, 127.0.0.1
 *
 * Protocol (from Dolphin's Source/Core/Core/HW/SI/SI_DeviceGBA.cpp and
 * mGBA's src/gba/sio/dolphin.c — reimplemented here, not copied):
 *   - TCP server on 127.0.0.1:54970 (data) and 127.0.0.1:49420 (clock).
 *   - The emulator connects TO us (we are the server, like Dolphin).
 *   - Wire format (server -> emulator):
 *       CMD_STATUS   (0x00) : 1 byte command,            3-byte response
 *       CMD_READ_GBA (0x14) : 1 byte command,            5-byte response
 *       CMD_WRITE_GBA(0x15) : 5 bytes (cmd + 4 data),    1-byte response
 *       CMD_RESET    (0xFF) : 1 byte command,            3-byte response
 *   - 4-byte big-endian clock slices are sent on the clock port ahead of
 *     commands; mGBA will not process commands until it receives clock data.
 *
 * Response validation mirrors Nintendo's SDK (src/static/dolphin/gba/GBA.c):
 *   - status/reset: resp[0]==0 && resp[1]==4, status = resp[2] & 0x3A
 *   - read:  4 data bytes + status = resp[4] & 0x3A
 *   - write: status = resp[0] & 0x3A
 *
 * Multiboot (GBAJoyBootAsync): "virtual multiboot". A faithful reimplementation
 * of Nintendo's multiboot master handshake is not possible here — the session
 * key schedule runs as DSP microcode (Jac_DSPagbDecodeAsync) with no PC
 * equivalent. Instead, the program image is written to disk and the peer is
 * assumed to already be running it (the user loads the dumped .bin in mGBA as
 * a multiboot image, then connects). The post-boot custom-protocol traffic
 * (mGcgba_Send/mGcgba_Recv) is real JOYBUS and flows unmodified over this
 * transport. gba_derive_session_keys() marks the exact seam where a real
 * handshake could be dropped in later.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdarg.h>

#ifdef _WIN32
#  include <winsock2.h>
#  include <ws2tcpip.h>
#  include <windows.h>
#  include <direct.h>
   typedef SOCKET sock_t;
#  define SOCK_INVALID INVALID_SOCKET
#  define sock_close closesocket
#  define sock_errno WSAGetLastError()
#  define SOCK_EWOULD WSAEWOULDBLOCK
#else
#  include <unistd.h>
#  include <errno.h>
#  include <fcntl.h>
#  include <pthread.h>
#  include <sys/types.h>
#  include <sys/socket.h>
#  include <sys/select.h>
#  include <netinet/in.h>
#  include <netinet/tcp.h>
#  include <arpa/inet.h>
   typedef int sock_t;
#  define SOCK_INVALID (-1)
#  define sock_close close
#  define sock_errno errno
#  define SOCK_EWOULD EWOULDBLOCK
#endif

/* ---- types matching the decomp (types.h) ---- */
typedef signed int s32;
typedef unsigned char u8;

/* ---- SDK constants (include/GBA/gba.h) ---- */
#define GBA_JSTAT_MASK 0x3A
#define GBA_READY 0
#define GBA_NOT_READY 1
#define GBA_BUSY 2
#define GBA_JOYBOOT_UNKNOWN_STATE 3
#define GBA_JOYBOOT_ERR_INVALID 4
#define GBA_JOYBOOT_PROGRAM_SIZE_MAX 0x40000
typedef void (*GBACallback)(s32 chan, s32 ret);

/* ---- JOYBUS commands (SI_Device.h EBufferCommands) ---- */
#define JOY_CMD_STATUS 0x00
#define JOY_CMD_READ   0x14
#define JOY_CMD_WRITE  0x15
#define JOY_CMD_RESET  0xFF

/* ---- Dolphin's TCP ports ---- */
#define JOYBUS_DATA_PORT 54970   /* 0xd6ba */
#define JOYBUS_CLOCK_PORT 49420  /* 0xc10c */

#define XFER_TIMEOUT_MS 2000
#define BOOT_PEER_TIMEOUT_S 120

static sock_t g_listen_data = SOCK_INVALID;
static sock_t g_listen_clock = SOCK_INVALID;
static sock_t g_data = SOCK_INVALID;
static sock_t g_clock = SOCK_INVALID;
static int g_net_init = 0;

/* virtual-multiboot state */
static int g_boot_waiting = 0;      /* GBAJoyBootAsync armed, waiting for peer */
static GBACallback g_boot_cb = 0;
static s32 g_boot_chan = 0;
static u8 *g_boot_status = 0;

static void joybus_log(const char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    fprintf(stderr, "[pc_gba_joybus] ");
    vfprintf(stderr, fmt, ap);
    fprintf(stderr, "\n");
    va_end(ap);
}

static void sock_nonblock(sock_t s) {
#ifdef _WIN32
    u_long m = 1;
    ioctlsocket(s, FIONBIO, &m);
#else
    int f = fcntl(s, F_GETFL, 0);
    fcntl(s, F_SETFL, f | O_NONBLOCK);
#endif
}

static sock_t listen_on(int port) {
    sock_t s = socket(AF_INET, SOCK_STREAM, 0);
    struct sockaddr_in a;
    int one = 1;
    if (s == SOCK_INVALID) return SOCK_INVALID;
#ifdef _WIN32
    (void)one;
#else
    setsockopt(s, SOL_SOCKET, SO_REUSEADDR, &one, sizeof(one));
#endif
    memset(&a, 0, sizeof(a));
    a.sin_family = AF_INET;
    a.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    a.sin_port = htons((unsigned short)port);
    if (bind(s, (struct sockaddr *)&a, sizeof(a)) != 0 ||
        listen(s, 4) != 0) {
        sock_close(s);
        return SOCK_INVALID;
    }
    sock_nonblock(s);
    return s;
}

/* Accept any pending clients (lazy accept, no threads needed). */
static void joybus_accept(void) {
    sock_t c;
    struct sockaddr_in a;
#ifdef _WIN32
    int alen = sizeof(a);
#else
    socklen_t alen = sizeof(a);
#endif
    if (g_listen_data != SOCK_INVALID) {
        for (;;) {
            c = accept(g_listen_data, (struct sockaddr *)&a, &alen);
            if (c == SOCK_INVALID) break;
            if (g_data != SOCK_INVALID) sock_close(g_data);
            g_data = c;
            sock_nonblock(g_data);
            joybus_log("emulator connected (data)");
        }
    }
    if (g_listen_clock != SOCK_INVALID) {
        for (;;) {
            c = accept(g_listen_clock, (struct sockaddr *)&a, &alen);
            if (c == SOCK_INVALID) break;
            if (g_clock != SOCK_INVALID) sock_close(g_clock);
            g_clock = c;
            sock_nonblock(g_clock);
        }
    }
}

static int joybus_send_all(sock_t s, const u8 *buf, int len) {
    int sent = 0;
    while (sent < len) {
        int n = (int)send(s, (const char *)buf + sent, len - sent, 0);
        if (n <= 0) {
            int e = sock_errno;
#ifdef _WIN32
            if (e == WSAEWOULDBLOCK) continue;
#else
            if (e == EWOULDBLOCK || e == EINTR) continue;
#endif
            return -1;
        }
        sent += n;
    }
    return 0;
}

/* Feed mGBA's clock so it processes commands. Slice ~= 1/60s in
 * Dolphin's units (16777216 per second). */
static void joybus_clock_tick(void) {
    if (g_clock == SOCK_INVALID) return;
    /* ~1/60s worth of slice per command keeps the peer's event loop moving */
    unsigned slice = 16777216 / 60;
    u8 b[4];
    b[0] = (u8)(slice >> 24);
    b[1] = (u8)(slice >> 16);
    b[2] = (u8)(slice >> 8);
    b[3] = (u8)(slice);
    if (joybus_send_all(g_clock, b, 4) != 0) {
        sock_close(g_clock);
        g_clock = SOCK_INVALID;
    }
}

/* One JOYBUS transaction. Returns 0 on success with rx_len bytes in rx,
 * -1 on any failure (peer gone, timeout, short read). */
static int joybus_xfer(u8 cmd, const u8 *tx, int tx_len, u8 *rx, int rx_len) {
    u8 out[5];
    int got = 0;
    int waits;

    joybus_accept();
    if (g_data == SOCK_INVALID) return -1;

    /* Dolphin flushes stale replies before each command */
    {
        u8 tmp[32];
        for (;;) {
            int n = (int)recv(g_data, (char *)tmp, sizeof(tmp), 0);
            if (n <= 0) break;
        }
    }

    joybus_clock_tick();

    out[0] = cmd;
    if (tx_len > 0 && tx != 0) memcpy(out + 1, tx, (size_t)tx_len);
    if (joybus_send_all(g_data, out, 1 + tx_len) != 0) {
        sock_close(g_data);
        g_data = SOCK_INVALID;
        return -1;
    }

    /* Wait for the full response, feeding the clock while we wait */
    waits = 0;
    while (got < rx_len && waits * 50 < XFER_TIMEOUT_MS) {
        fd_set rf;
        struct timeval tv;
        FD_ZERO(&rf);
        FD_SET(g_data, &rf);
        tv.tv_sec = 0;
        tv.tv_usec = 50000;
        if (select((int)g_data + 1, &rf, 0, 0, &tv) > 0) {
            int n = (int)recv(g_data, (char *)rx + got, rx_len - got, 0);
            if (n <= 0) {
                sock_close(g_data);
                g_data = SOCK_INVALID;
                return -1;
            }
            got += n;
        } else {
            joybus_clock_tick(); /* keep the peer's event loop alive */
        }
        waits++;
    }
    return (got == rx_len) ? 0 : -1;
}

void GBAInit(void) {
    if (g_net_init) return;
#ifdef _WIN32
    {
        WSADATA wd;
        if (WSAStartup(MAKEWORD(2, 2), &wd) != 0) {
            joybus_log("WSAStartup failed");
            return;
        }
    }
#endif
    g_listen_data = listen_on(JOYBUS_DATA_PORT);
    g_listen_clock = listen_on(JOYBUS_CLOCK_PORT);
    if (g_listen_data == SOCK_INVALID || g_listen_clock == SOCK_INVALID) {
        joybus_log("could not listen on 127.0.0.1:%d/%d (another instance running?)",
                   JOYBUS_DATA_PORT, JOYBUS_CLOCK_PORT);
    } else {
        joybus_log("listening on 127.0.0.1:%d (data) and :%d (clock) — "
                   "in mGBA: File -> Connect to Dolphin",
                   JOYBUS_DATA_PORT, JOYBUS_CLOCK_PORT);
    }
    g_net_init = 1;
}

s32 GBAGetStatus(s32 chan, u8 *status) {
    u8 rx[3];
    (void)chan;
    if (joybus_xfer(JOY_CMD_STATUS, 0, 0, rx, 3) != 0) return GBA_NOT_READY;
    /* ShortCommandProc: input[0]==0 && input[1]==4 */
    if (rx[0] != 0 || rx[1] != 4) return GBA_NOT_READY;
    if (status) *status = rx[2] & GBA_JSTAT_MASK;
    return GBA_READY;
}

s32 GBAReset(s32 chan, u8 *status) {
    u8 rx[3];
    (void)chan;
    if (joybus_xfer(JOY_CMD_RESET, 0, 0, rx, 3) != 0) return GBA_NOT_READY;
    if (rx[0] != 0 || rx[1] != 4) return GBA_NOT_READY;
    if (status) *status = rx[2] & GBA_JSTAT_MASK;
    return GBA_READY;
}

s32 GBARead(s32 chan, u8 *dst, u8 *status) {
    u8 rx[5];
    (void)chan;
    if (!dst) return GBA_NOT_READY;
    if (joybus_xfer(JOY_CMD_READ, 0, 0, rx, 5) != 0) return GBA_NOT_READY;
    memcpy(dst, rx, 4);
    if (status) *status = rx[4] & GBA_JSTAT_MASK;
    return GBA_READY;
}

s32 GBAWrite(s32 chan, u8 *src, u8 *status) {
    u8 rx[1];
    (void)chan;
    if (!src) return GBA_NOT_READY;
    if (joybus_xfer(JOY_CMD_WRITE, src, 4, rx, 1) != 0) return GBA_NOT_READY;
    if (status) *status = rx[0] & GBA_JSTAT_MASK;
    return GBA_READY;
}

s32 GBAGetProcessStatus(s32 chan, u8 *percentp) {
    (void)chan;
    /* Virtual multiboot is "busy" until the peer connects. */
    if (g_boot_waiting) {
        joybus_accept();
        if (g_data != SOCK_INVALID) {
            /* Peer arrived: complete the virtual boot. */
            g_boot_waiting = 0;
            if (g_boot_status) *g_boot_status = 0;
            if (percentp) *percentp = 100;
            if (g_boot_cb) {
                GBACallback cb = g_boot_cb;
                g_boot_cb = 0;
                cb(g_boot_chan, GBA_READY);
            }
            return GBA_READY;
        }
        if (percentp) *percentp = 0;
        return GBA_BUSY;
    }
    if (percentp) *percentp = 100;
    return GBA_READY;
}

/* NOTE on the multiboot handshake: a faithful reimplementation of Nintendo's
 * multiboot master is not possible here — the session key schedule runs as
 * DSP microcode (Jac_DSPagbDecodeAsync in the SDK) with no PC equivalent.
 * The virtual-multiboot path below is the pragmatic substitute; if the key
 * schedule is ever reproduced, it slots in right here. */

static void gba_dump_program(const u8 *prog, s32 len, char *path_out, size_t path_sz) {
#ifdef _WIN32
    char tmp[MAX_PATH];
    DWORD n = GetTempPathA(sizeof(tmp), tmp);
    if (n == 0 || n >= sizeof(tmp)) strcpy(tmp, ".\\");
    _snprintf(path_out, path_sz, "%srustimal_gba_boot.bin", tmp);
#else
    snprintf(path_out, path_sz, "/tmp/rustimal_gba_boot.bin");
#endif
    {
        FILE *f = fopen(path_out, "wb");
        if (f) {
            fwrite(prog, 1, (size_t)len, f);
            fclose(f);
        }
    }
}

/* Best-effort: open the dumped program in mGBA so the user only has to click
 * "Connect to Dolphin". Never fatal if it fails. */
static void gba_try_launch_emulator(const char *bin_path) {
    const char *emu = getenv("PC_GBA_EMULATOR");
    char cmd[1024];
    if (!emu || !*emu) emu = "mgba-qt";
#ifdef _WIN32
    _snprintf(cmd, sizeof(cmd), "start \"\" \"%s\" \"%s\"", emu, bin_path);
#else
    snprintf(cmd, sizeof(cmd), "\"%s\" \"%s\" >/dev/null 2>&1 &", emu, bin_path);
#endif
    if (getenv("PC_GBA_AUTO_LAUNCH") && strcmp(getenv("PC_GBA_AUTO_LAUNCH"), "0") == 0)
        return;
    if (system(cmd) != 0) {
        joybus_log("could not auto-launch emulator; load %s in mGBA manually", bin_path);
    } else {
        joybus_log("launched %s with %s", emu, bin_path);
    }
}

s32 GBAJoyBootAsync(s32 chan, s32 palette_color, s32 palette_speed, u8 *programp,
                    s32 length, u8 *status, GBACallback callback) {
    char path[512];
    (void)palette_color;
    (void)palette_speed;

    /* Mirror the SDK's argument validation (GBAJoyBoot.c). */
    if ((chan & ~3) != 0) return GBA_JOYBOOT_ERR_INVALID;
    if (length == 0 || length > GBA_JOYBOOT_PROGRAM_SIZE_MAX) return GBA_JOYBOOT_ERR_INVALID;
    if (palette_speed < -4 || palette_speed > 4) return GBA_JOYBOOT_ERR_INVALID;
    if (palette_color < 0 || palette_color > 6) return GBA_JOYBOOT_ERR_INVALID;
    if (programp[0xAC] == 0 || programp[0xAD] == 0 ||
        programp[0xAE] == 0 || programp[0xAF] == 0)
        return GBA_JOYBOOT_ERR_INVALID;

    /* Virtual multiboot: hand the program to the user/peer out-of-band. */
    gba_dump_program(programp, length, path, sizeof(path));
    joybus_log("virtual multiboot: wrote %d bytes to %s", (int)length, path);
    joybus_log("load that file in mGBA (as a multiboot image), then File -> Connect to Dolphin");
    gba_try_launch_emulator(path);

    g_boot_waiting = 1;
    g_boot_cb = callback;
    g_boot_chan = chan;
    g_boot_status = status;
    /* Like the SDK, report kickoff success; completion arrives via the
     * callback once the peer connects (see GBAGetProcessStatus). */
    return GBA_READY;
}
