//! The arithmetic of JavaScript where Rust's differs from it, for code ported from Pele that
//! must come out as Pele's does to the last bit.
//!
//! Math.sin, Math.cos, Math.atan2, Math.acos, Math.asin and Math.log10 are worked out as V8
//! works them out: fdlibm's functions, from V8's src/base/ieee754.cc, which Node builds
//! without V8_USE_LIBM_TRIG_FUNCTIONS and so without glibc's. Node on arm64 gives what they
//! give compiled by clang with -ffp-contract=on, its default, which fuses a product into the
//! sum or difference it is written in, in the same expression, into one multiply-add: the
//! product on the left of the operator if there is one, else the one on its right. `mul_add`
//! writes out each of those here, and every other product is rounded on its own. Built
//! without fused multiply-adds, as for x86-64, V8 gives other last bits for about one input
//! in two hundred.

// The code keeps fdlibm's shape, to be read beside it: its constants as it writes them, to
// more digits than a double holds and where Rust has them by name, x - x for NaN of an
// infinity, and its variables and loop counters.
#![allow(
    clippy::approx_constant,
    clippy::eq_op,
    clippy::excessive_precision,
    clippy::explicit_counter_loop,
    clippy::needless_late_init
)]

/// Math.max of two numbers: NaN if either is, and +0 over -0. Rust's `f64::max` takes the
/// number over NaN, and may give either zero.
pub(crate) fn max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == b {
        if a.is_sign_negative() { b } else { a }
    } else if a > b {
        a
    } else {
        b
    }
}

/// Math.min of two numbers: NaN if either is, and -0 under +0.
pub(crate) fn min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == b {
        if a.is_sign_negative() { a } else { b }
    } else if a < b {
        a
    } else {
        b
    }
}

/// Math.round: the nearest integer, halves up, and zero with the sign of x where that is the
/// nearest, as Math.round(-0.25) is -0. It is not (x + 0.5).floor(), which rounds
/// 0.49999999999999994 up, as the sum is 1.
pub(crate) fn round(x: f64) -> f64 {
    let mut r = x.floor();
    if x - r >= 0.5 {
        r += 1.0;
    }
    if r == 0.0 { 0.0f64.copysign(x) } else { r }
}

/// Math.sign: -1, 1, or x itself when it is zero, of either sign, or NaN.
pub(crate) fn sign(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        x
    }
}

/// x || 1.
pub(crate) fn or1(x: f64) -> f64 {
    if x == 0.0 || x.is_nan() { 1.0 } else { x }
}

/// Math.sign(x) || 1.
pub(crate) fn sign_or1(x: f64) -> f64 {
    or1(sign(x))
}

/// Math.hypot of two numbers as V8 works it out, which is not always the last bit of Rust's
/// hypot: each is divided by the larger, their squares are summed with Kahan's
/// compensation, and the root is scaled back.
pub(crate) fn hypot(a: f64, b: f64) -> f64 {
    let (a, b) = (a.abs(), b.abs());
    if a.is_infinite() || b.is_infinite() {
        return f64::INFINITY;
    }
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    let m = a.max(b);
    if m == 0.0 {
        return 0.0;
    }
    let (mut sum, mut comp) = (0.0f64, 0.0f64);
    for v in [a, b] {
        let n = v / m;
        let summand = n * n - comp;
        let prelim = sum + summand;
        comp = (prelim - sum) - summand;
        sum = prelim;
    }
    sum.sqrt() * m
}

/// Math.pow(10, n) for a whole n, which V8 rounds correctly, as a literal 1eN is. For an n
/// that is not whole it is Rust's powf, which need not be V8's to the last bit.
pub(crate) fn pow10(n: f64) -> f64 {
    if n != n.trunc() || n.is_infinite() {
        return 10f64.powf(n);
    }
    if n > 400.0 {
        return f64::INFINITY;
    }
    if n < -400.0 {
        return 0.0;
    }
    format!("1e{}", n as i32).parse().unwrap_or(f64::NAN)
}

/// The high word of x's bits, as fdlibm's GET_HIGH_WORD.
fn high(x: f64) -> i32 {
    (x.to_bits() >> 32) as i32
}

/// The low word of x's bits, as fdlibm's GET_LOW_WORD.
fn low(x: f64) -> u32 {
    x.to_bits() as u32
}

/// The number of a high word and a low word, as fdlibm's INSERT_WORDS.
fn words(hi: i32, lo: u32) -> f64 {
    f64::from_bits((u64::from(hi as u32) << 32) | u64::from(lo))
}

/// x·2^n, as C's scalbn.
fn ldexp(mut x: f64, mut n: i32) -> f64 {
    let p = |e: i32| f64::from_bits(((0x3ff + e) as u64) << 52);
    if n > 1023 {
        x *= p(1023);
        n -= 1023;
        if n > 1023 {
            x *= p(1023);
            n = (n - 1023).min(1023);
        }
    } else if n < -1022 {
        // Scaled down in two steps, by 2^-969 at a time, so that the result is rounded once
        // where it is subnormal.
        x *= p(-1022) * p(53);
        n += 1022 - 53;
        if n < -1022 {
            x *= p(-1022) * p(53);
            n = (n + 1022 - 53).max(-1022);
        }
    }
    x * p(n)
}

/// Math.sin, as fdlibm's sin does.
pub(crate) fn sin(x: f64) -> f64 {
    let ix = high(x) & 0x7fffffff;
    if ix <= 0x3fe921fb {
        return kernel_sin(x, 0.0, false);
    }
    if ix >= 0x7ff00000 {
        return x - x; // NaN, for Inf or NaN
    }
    let (n, y0, y1) = rem_pio2(x);
    match n & 3 {
        0 => kernel_sin(y0, y1, true),
        1 => kernel_cos(y0, y1),
        2 => -kernel_sin(y0, y1, true),
        _ => -kernel_cos(y0, y1),
    }
}

/// Math.cos, as fdlibm's cos does.
pub(crate) fn cos(x: f64) -> f64 {
    let ix = high(x) & 0x7fffffff;
    if ix <= 0x3fe921fb {
        return kernel_cos(x, 0.0);
    }
    if ix >= 0x7ff00000 {
        return x - x; // NaN, for Inf or NaN
    }
    let (n, y0, y1) = rem_pio2(x);
    match n & 3 {
        0 => kernel_cos(y0, y1),
        1 => -kernel_sin(y0, y1, true),
        2 => -kernel_cos(y0, y1),
        _ => kernel_sin(y0, y1, true),
    }
}

/// sin(x+y) for |x| up to about π/4, y the tail of x, which is taken to be 0 unless tail is
/// set, as fdlibm's __kernel_sin does.
fn kernel_sin(x: f64, y: f64, tail: bool) -> f64 {
    const HALF: f64 = 5.00000000000000000000e-01;
    const S1: f64 = -1.66666666666666324348e-01;
    const S2: f64 = 8.33333333332248946124e-03;
    const S3: f64 = -1.98412698298579493134e-04;
    const S4: f64 = 2.75573137070700676789e-06;
    const S5: f64 = -2.50507602534068634195e-08;
    const S6: f64 = 1.58969099521155010221e-10;
    let ix = high(x) & 0x7fffffff;
    if ix < 0x3e400000 {
        return x; // |x| < 2**-27
    }
    let z = x * x;
    let v = z * x;
    let r = z.mul_add(z.mul_add(z.mul_add(z.mul_add(S6, S5), S4), S3), S2);
    if !tail {
        return v.mul_add(z.mul_add(r, S1), x);
    }
    x - (-v).mul_add(S1, z.mul_add(HALF.mul_add(y, -(v * r)), -y))
}

/// cos(x+y) for |x| up to about π/4, y the tail of x, as fdlibm's __kernel_cos does.
fn kernel_cos(x: f64, y: f64) -> f64 {
    const C1: f64 = 4.16666666666666019037e-02;
    const C2: f64 = -1.38888888888741095749e-03;
    const C3: f64 = 2.48015872894767294178e-05;
    const C4: f64 = -2.75573143513906633035e-07;
    const C5: f64 = 2.08757232129817482790e-09;
    const C6: f64 = -1.13596475577881948265e-11;
    let ix = high(x) & 0x7fffffff;
    if ix < 0x3e400000 {
        return 1.0; // |x| < 2**-27
    }
    let z = x * x;
    let r = z * z.mul_add(
        z.mul_add(z.mul_add(z.mul_add(z.mul_add(C6, C5), C4), C3), C2),
        C1,
    );
    if ix < 0x3fd33333 {
        // |x| < 0.3
        return 1.0 - 0.5f64.mul_add(z, -z.mul_add(r, -(x * y)));
    }
    let qx = if ix > 0x3fe90000 {
        0.28125 // |x| > 0.78125
    } else {
        words(ix - 0x00200000, 0) // |x|/4
    };
    let iz = 0.5f64.mul_add(z, -qx);
    let a = 1.0 - qx;
    a - (iz - z.mul_add(r, -(x * y)))
}

/// 2/π in 24-bit pieces, 396 hexadecimal digits of it.
const TWO_OVER_PI: [i32; 66] = [
    0xA2F983, 0x6E4E44, 0x1529FC, 0x2757D1, 0xF534DD, 0xC0DB62, 0x95993C, 0x439041, 0xFE5163,
    0xABDEBB, 0xC561B7, 0x246E3A, 0x424DD2, 0xE00649, 0x2EEA09, 0xD1921C, 0xFE1DEB, 0x1CB129,
    0xA73EE8, 0x8235F5, 0x2EBB44, 0x84E99C, 0x7026B4, 0x5F7E41, 0x3991D6, 0x398353, 0x39F49C,
    0x845F8B, 0xBDF928, 0x3B1FF8, 0x97FFDE, 0x05980F, 0xEF2F11, 0x8B5A0A, 0x6D1F6D, 0x367ECF,
    0x27CB09, 0xB74F46, 0x3F669E, 0x5FEA2D, 0x7527BA, 0xC7EBE5, 0xF17B3D, 0x0739F7, 0x8A5292,
    0xEA6BFB, 0x5FB11F, 0x8D5D08, 0x560330, 0x46FC7B, 0x6BABF0, 0xCFBC20, 0x9AF436, 0x1DA9E3,
    0x91615E, 0xE61B08, 0x659985, 0x5F14A0, 0x68408D, 0xFFD880, 0x4D7327, 0x310606, 0x1556CA,
    0x73A8C9, 0x60E27B, 0xC08C6B,
];

/// The high words of n·π/2 for n from 1 to 32.
const NPIO2_HW: [i32; 32] = [
    0x3FF921FB, 0x400921FB, 0x4012D97C, 0x401921FB, 0x401F6A7A, 0x4022D97C, 0x4025FDBB, 0x402921FB,
    0x402C463A, 0x402F6A7A, 0x4031475C, 0x4032D97C, 0x40346B9C, 0x4035FDBB, 0x40378FDB, 0x403921FB,
    0x403AB41B, 0x403C463A, 0x403DD85A, 0x403F6A7A, 0x40407E4C, 0x4041475C, 0x4042106C, 0x4042D97C,
    0x4043A28C, 0x40446B9C, 0x404534AC, 0x4045FDBB, 0x4046C6CB, 0x40478FDB, 0x404858EB, 0x404921FB,
];

/// x less the nearest multiple n of π/2, as y0+y1, and n, as fdlibm's __ieee754_rem_pio2
/// does: with π/2 in three pieces of 33 bits for |x| up to 2^19·π/2, and with
/// __kernel_rem_pio2 beyond.
fn rem_pio2(x: f64) -> (i32, f64, f64) {
    const HALF: f64 = 5.00000000000000000000e-01;
    const TWO24: f64 = 1.67772160000000000000e+07;
    const INVPIO2: f64 = 6.36619772367581382433e-01;
    const PIO2_1: f64 = 1.57079632673412561417e+00;
    const PIO2_1T: f64 = 6.07710050650619224932e-11;
    const PIO2_2: f64 = 6.07710050630396597660e-11;
    const PIO2_2T: f64 = 2.02226624879595063154e-21;
    const PIO2_3: f64 = 2.02226624871116645580e-21;
    const PIO2_3T: f64 = 8.47842766036889956997e-32;
    let hx = high(x);
    let ix = hx & 0x7fffffff;
    if ix <= 0x3fe921fb {
        return (0, x, 0.0); // |x| ~<= π/4, no need for reduction
    }
    if ix < 0x4002d97c {
        // |x| < 3π/4, special case with n = ±1
        let (y0, y1);
        if hx > 0 {
            let mut z = x - PIO2_1;
            if ix != 0x3ff921fb {
                // 33+53 bits of π are enough
                y0 = z - PIO2_1T;
                y1 = (z - y0) - PIO2_1T;
            } else {
                // near π/2, use 33+33+53 bits of π
                z -= PIO2_2;
                y0 = z - PIO2_2T;
                y1 = (z - y0) - PIO2_2T;
            }
            return (1, y0, y1);
        }
        let mut z = x + PIO2_1;
        if ix != 0x3ff921fb {
            y0 = z + PIO2_1T;
            y1 = (z - y0) + PIO2_1T;
        } else {
            z += PIO2_2;
            y0 = z + PIO2_2T;
            y1 = (z - y0) + PIO2_2T;
        }
        return (-1, y0, y1);
    }
    if ix <= 0x413921fb {
        // |x| ~<= 2^19·(π/2), medium size
        let mut t = x.abs();
        let n = t.mul_add(INVPIO2, HALF) as i32;
        let fn_ = f64::from(n);
        let mut r = (-fn_).mul_add(PIO2_1, t);
        let mut w = fn_ * PIO2_1T; // first round, good to 85 bits
        let y0;
        if n < 32 && ix != NPIO2_HW[(n - 1) as usize] {
            y0 = r - w; // quick check, no cancellation
        } else {
            let j = ix >> 20;
            let mut y = r - w;
            let mut i = j - ((high(y) >> 20) & 0x7ff);
            if i > 16 {
                // second iteration, good to 118 bits
                t = r;
                w = fn_ * PIO2_2;
                r = t - w;
                w = fn_.mul_add(PIO2_2T, -((t - r) - w));
                y = r - w;
                i = j - ((high(y) >> 20) & 0x7ff);
                if i > 49 {
                    // third iteration, 151 bits, for every case
                    t = r;
                    w = fn_ * PIO2_3;
                    r = t - w;
                    w = fn_.mul_add(PIO2_3T, -((t - r) - w));
                    y = r - w;
                }
            }
            y0 = y;
        }
        let y1 = (r - y0) - w;
        return if hx < 0 { (-n, -y0, -y1) } else { (n, y0, y1) };
    }
    // All other, large, arguments.
    if ix >= 0x7ff00000 {
        let y = x - x; // Inf or NaN
        return (0, y, y);
    }
    // z = scalbn(|x|, ilogb(x)-23)
    let e0 = (ix >> 20) - 1046;
    let mut z = words(ix - (e0 << 20), low(x));
    let mut tx = [0.0; 3];
    for t in tx.iter_mut().take(2) {
        *t = f64::from(z as i32);
        z = (z - *t) * TWO24;
    }
    tx[2] = z;
    let mut nx = 3;
    while tx[nx - 1] == 0.0 {
        nx -= 1; // skip zero terms
    }
    let (n, y0, y1) = kernel_rem_pio2(&tx[..nx], e0);
    if hx < 0 { (-n, -y0, -y1) } else { (n, y0, y1) }
}

/// x·2^(e0-24·i) summed over the 24-bit pieces x[i], less the nearest multiple of π/2, as
/// y0+y1, and that multiple modulo 8, as fdlibm's __kernel_rem_pio2 does with prec 2, the
/// precision __ieee754_rem_pio2 asks of it.
fn kernel_rem_pio2(x: &[f64], e0: i32) -> (i32, f64, f64) {
    const TWO24: f64 = 1.67772160000000000000e+07;
    const TWON24: f64 = 5.96046447753906250000e-08;
    const JK: i32 = 4; // init_jk[prec] for prec 2
    const JP: i32 = JK;
    const PIO2: [f64; 8] = [
        1.57079625129699707031e+00,
        7.54978941586159635335e-08,
        5.39030252995776476554e-15,
        3.28200341580791294123e-22,
        1.27065575308067607349e-29,
        1.22933308981111328932e-36,
        2.73370053816464559624e-44,
        2.16741683877804819444e-51,
    ];
    let mut iq = [0i32; 20];
    let (mut f, mut fq, mut q) = ([0.0f64; 20], [0.0f64; 20], [0.0f64; 20]);
    let (mut z, mut fw);
    let (mut n, mut ih);
    let mut jz;
    let u = |i: i32| i as usize;

    // Determine jx, jv and q0; note that 3 > q0.
    let jx = x.len() as i32 - 1;
    let jv = ((e0 - 3) / 24).max(0);
    let mut q0 = e0 - 24 * (jv + 1);

    // Set up f[0] to f[jx+jk] where f[jx+jk] = ipio2[jv+jk].
    let mut j = jv - jx;
    for i in 0..=jx + JK {
        f[u(i)] = if j < 0 {
            0.0
        } else {
            f64::from(TWO_OVER_PI[u(j)])
        };
        j += 1;
    }

    // Compute q[0], q[1], … q[jk].
    for i in 0..=JK {
        fw = 0.0;
        for j in 0..=jx {
            fw = x[u(j)].mul_add(f[u(jx + i - j)], fw);
        }
        q[u(i)] = fw;
    }

    jz = JK;
    loop {
        // Distill q[] into iq[], in reverse.
        z = q[u(jz)];
        let (mut i, mut j) = (0, jz);
        while j > 0 {
            fw = f64::from((TWON24 * z) as i32);
            iq[u(i)] = (-TWO24).mul_add(fw, z) as i32;
            z = q[u(j - 1)] + fw;
            i += 1;
            j -= 1;
        }

        // Compute n.
        z = ldexp(z, q0); // the actual value of z
        z = (-8.0f64).mul_add((z * 0.125).floor(), z); // trim off integers >= 8
        n = z as i32;
        z -= f64::from(n);
        ih = 0;
        if q0 > 0 {
            // iq[jz-1] is needed to determine n
            let i = iq[u(jz - 1)] >> (24 - q0);
            n += i;
            iq[u(jz - 1)] -= i << (24 - q0);
            ih = iq[u(jz - 1)] >> (23 - q0);
        } else if q0 == 0 {
            ih = iq[u(jz - 1)] >> 23;
        } else if z >= 0.5 {
            ih = 2;
        }

        if ih > 0 {
            // q > 0.5
            n += 1;
            let mut carry = 0;
            for v in iq.iter_mut().take(u(jz)) {
                // compute 1-q
                let j = *v;
                if carry == 0 {
                    if j != 0 {
                        carry = 1;
                        *v = 0x1000000 - j;
                    }
                } else {
                    *v = 0xffffff - j;
                }
            }
            if q0 > 0 {
                // rare case: chance is 1 in 12
                match q0 {
                    1 => iq[u(jz - 1)] &= 0x7fffff,
                    2 => iq[u(jz - 1)] &= 0x3fffff,
                    _ => {}
                }
            }
            if ih == 2 {
                z = 1.0 - z;
                if carry != 0 {
                    z -= ldexp(1.0, q0);
                }
            }
        }

        // Check if recomputation is needed.
        if z == 0.0 {
            let mut j = 0;
            for i in (JK..jz).rev() {
                j |= iq[u(i)];
            }
            if j == 0 {
                // need recomputation
                let mut k = 1;
                while JK >= k && iq[u(JK - k)] == 0 {
                    k += 1; // k = number of terms needed
                }
                for i in jz + 1..=jz + k {
                    // add q[jz+1] to q[jz+k]
                    f[u(jx + i)] = f64::from(TWO_OVER_PI[u(jv + i)]);
                    fw = 0.0;
                    for j in 0..=jx {
                        fw = x[u(j)].mul_add(f[u(jx + i - j)], fw);
                    }
                    q[u(i)] = fw;
                }
                jz += k;
                continue;
            }
        }
        break;
    }

    // Chop off zero terms.
    if z == 0.0 {
        jz -= 1;
        q0 -= 24;
        while iq[u(jz)] == 0 {
            jz -= 1;
            q0 -= 24;
        }
    } else {
        // break z into 24 bits if necessary
        z = ldexp(z, -q0);
        if z >= TWO24 {
            fw = f64::from((TWON24 * z) as i32);
            iq[u(jz)] = (-TWO24).mul_add(fw, z) as i32;
            jz += 1;
            q0 += 24;
            iq[u(jz)] = fw as i32;
        } else {
            iq[u(jz)] = z as i32;
        }
    }

    // Convert the integer "bit" chunks to floating-point values.
    fw = ldexp(1.0, q0);
    for i in (0..=jz).rev() {
        q[u(i)] = fw * f64::from(iq[u(i)]);
        fw *= TWON24;
    }

    // Compute PIo2[0, …, jp]·q[jz, …, 0].
    for i in (0..=jz).rev() {
        fw = 0.0;
        let mut k = 0;
        while k <= JP && k <= jz - i {
            fw = PIO2[u(k)].mul_add(q[u(i + k)], fw);
            k += 1;
        }
        fq[u(jz - i)] = fw;
    }

    // Compress fq[] into y0 and y1.
    fw = 0.0;
    for i in (0..=jz).rev() {
        fw += fq[u(i)];
    }
    let y0 = if ih != 0 { -fw } else { fw };
    fw = fq[0] - fw;
    for i in 1..=jz {
        fw += fq[u(i)];
    }
    let y1 = if ih != 0 { -fw } else { fw };
    (n & 7, y0, y1)
}

/// What is added with these is added when the function runs, as they are volatile in
/// fdlibm's e_atan2.c.
const ATAN2_TINY: f64 = 1.0e-300;
const ATAN2_PI_LO: f64 = 1.2246467991473531772e-16;

/// Math.atan2(y, x), as fdlibm's __ieee754_atan2 does.
pub(crate) fn atan2(y: f64, x: f64) -> f64 {
    const PI_O4: f64 = 7.8539816339744827900e-01;
    const PI_O2: f64 = 1.5707963267948965580e+00;
    const PI: f64 = 3.1415926535897931160e+00;
    let (hx, lx) = (high(x), low(x));
    let (hy, ly) = (high(y), low(y));
    let ix = hx & 0x7fffffff;
    let iy = hy & 0x7fffffff;
    if (ix as u32 | ((lx | lx.wrapping_neg()) >> 31)) > 0x7ff00000
        || (iy as u32 | ((ly | ly.wrapping_neg()) >> 31)) > 0x7ff00000
    {
        return x + y; // x or y is NaN
    }
    if ((hx.wrapping_sub(0x3ff00000)) | lx as i32) == 0 {
        return atan(y); // x is 1
    }
    let mut m = ((hy >> 31) & 1) | ((hx >> 30) & 2); // 2·sign(x) + sign(y)

    if (iy as u32 | ly) == 0 {
        // y is 0
        return match m {
            0 | 1 => y,            // atan(±0, +anything) = ±0
            2 => PI + ATAN2_TINY,  // atan(+0, -anything) = π
            _ => -PI - ATAN2_TINY, // atan(-0, -anything) = -π
        };
    }
    if (ix as u32 | lx) == 0 {
        // x is 0
        return if hy < 0 {
            -PI_O2 - ATAN2_TINY
        } else {
            PI_O2 + ATAN2_TINY
        };
    }
    if ix == 0x7ff00000 {
        // x is Inf
        if iy == 0x7ff00000 {
            return match m {
                0 => PI_O4 + ATAN2_TINY,        // atan(+Inf, +Inf)
                1 => -PI_O4 - ATAN2_TINY,       // atan(-Inf, +Inf)
                2 => 3.0 * PI_O4 + ATAN2_TINY,  // atan(+Inf, -Inf)
                _ => -3.0 * PI_O4 - ATAN2_TINY, // atan(-Inf, -Inf)
            };
        }
        return match m {
            0 => 0.0,              // atan(+…, +Inf)
            1 => -0.0,             // atan(-…, +Inf)
            2 => PI + ATAN2_TINY,  // atan(+…, -Inf)
            _ => -PI - ATAN2_TINY, // atan(-…, -Inf)
        };
    }
    if iy == 0x7ff00000 {
        // y is Inf
        return if hy < 0 {
            -PI_O2 - ATAN2_TINY
        } else {
            PI_O2 + ATAN2_TINY
        };
    }

    // Compute y/x.
    let k = (iy - ix) >> 20;
    let z = if k > 60 {
        // |y/x| > 2^60
        m &= 1;
        0.5f64.mul_add(ATAN2_PI_LO, PI_O2)
    } else if hx < 0 && k < -60 {
        0.0 // 0 > |y|/x > -2^-60
    } else {
        atan((y / x).abs())
    };
    match m {
        0 => z,                      // atan(+, +)
        1 => -z,                     // atan(-, +)
        2 => PI - (z - ATAN2_PI_LO), // atan(+, -)
        _ => (z - ATAN2_PI_LO) - PI, // atan(-, -)
    }
}

/// Math.atan, as fdlibm's atan does.
pub(crate) fn atan(mut x: f64) -> f64 {
    const ATANHI: [f64; 4] = [
        4.63647609000806093515e-01, // atan(0.5) hi
        7.85398163397448278999e-01, // atan(1.0) hi
        9.82793723247329054082e-01, // atan(1.5) hi
        1.57079632679489655800e+00, // atan(inf) hi
    ];
    const ATANLO: [f64; 4] = [
        2.26987774529616870924e-17, // atan(0.5) lo
        3.06161699786838301793e-17, // atan(1.0) lo
        1.39033110312309984516e-17, // atan(1.5) lo
        6.12323399573676603587e-17, // atan(inf) lo
    ];
    const AT: [f64; 11] = [
        3.33333333333329318027e-01,
        -1.99999999998764832476e-01,
        1.42857142725034663711e-01,
        -1.11111104054623557880e-01,
        9.09088713343650656196e-02,
        -7.69187620504482999495e-02,
        6.66107313738753120669e-02,
        -5.83357013379057348645e-02,
        4.97687799461593236017e-02,
        -3.65315727442169155270e-02,
        1.62858201153657823623e-02,
    ];
    let hx = high(x);
    let ix = hx & 0x7fffffff;
    if ix >= 0x44100000 {
        // |x| >= 2^66
        if ix > 0x7ff00000 || (ix == 0x7ff00000 && low(x) != 0) {
            return x + x; // NaN
        }
        return if hx > 0 {
            ATANHI[3] + ATANLO[3]
        } else {
            -ATANHI[3] - ATANLO[3]
        };
    }
    let id: i32;
    if ix < 0x3fdc0000 {
        // |x| < 0.4375
        if ix < 0x3e400000 {
            return x; // |x| < 2^-27
        }
        id = -1;
    } else {
        x = x.abs();
        if ix < 0x3ff30000 {
            // |x| < 1.1875
            if ix < 0x3fe60000 {
                // 7/16 <= |x| < 11/16
                id = 0;
                x = 2.0f64.mul_add(x, -1.0) / (2.0 + x);
            } else {
                // 11/16 <= |x| < 19/16
                id = 1;
                x = (x - 1.0) / (x + 1.0);
            }
        } else if ix < 0x40038000 {
            // |x| < 2.4375
            id = 2;
            x = (x - 1.5) / 1.5f64.mul_add(x, 1.0);
        } else {
            // 2.4375 <= |x| < 2^66
            id = 3;
            x = -1.0 / x;
        }
    }
    // End of argument reduction.
    let z = x * x;
    let w = z * z;
    // Break the sum of aT[i]·z^(i+1) from i = 0 to 10 into odd and even polynomials.
    let s1 = z * w.mul_add(
        w.mul_add(
            w.mul_add(w.mul_add(w.mul_add(AT[10], AT[8]), AT[6]), AT[4]),
            AT[2],
        ),
        AT[0],
    );
    let s2 = w * w.mul_add(
        w.mul_add(w.mul_add(w.mul_add(AT[9], AT[7]), AT[5]), AT[3]),
        AT[1],
    );
    if id < 0 {
        return (-x).mul_add(s1 + s2, x);
    }
    let id = id as usize;
    let z = ATANHI[id] - (x.mul_add(s1 + s2, -ATANLO[id]) - x);
    if hx < 0 { -z } else { z }
}

// The coefficients of the rational approximations of fdlibm's e_acos.c and e_asin.c.
const PS0: f64 = 1.66666666666666657415e-01;
const PS1: f64 = -3.25565818622400915405e-01;
const PS2: f64 = 2.01212532134862925881e-01;
const PS3: f64 = -4.00555345006794114027e-02;
const PS4: f64 = 7.91534994289814532176e-04;
const PS5: f64 = 3.47933107596021167570e-05;
const QS1: f64 = -2.40339491173441421878e+00;
const QS2: f64 = 2.02094576023350569471e+00;
const QS3: f64 = -6.88283971605453293030e-01;
const QS4: f64 = 7.70381505559019352791e-02;
const PIO2_HI: f64 = 1.57079632679489655800e+00;
const PIO2_LO: f64 = 6.12323399573676603587e-17;

/// The numerator of the rational approximation of (asin(√z) - √z)/√z, as fdlibm writes it
/// out in each branch.
fn asin_p(z: f64) -> f64 {
    z * z.mul_add(
        z.mul_add(z.mul_add(z.mul_add(z.mul_add(PS5, PS4), PS3), PS2), PS1),
        PS0,
    )
}

/// The denominator of that approximation.
fn asin_q(z: f64) -> f64 {
    z.mul_add(z.mul_add(z.mul_add(z.mul_add(QS4, QS3), QS2), QS1), 1.0)
}

/// x with the low word of its bits cleared, as fdlibm's SET_LOW_WORD(x, 0).
fn high_part(x: f64) -> f64 {
    f64::from_bits(x.to_bits() & !0xffffffff)
}

/// Math.acos, as fdlibm's __ieee754_acos does.
pub(crate) fn acos(x: f64) -> f64 {
    const PI: f64 = 3.14159265358979311600e+00;
    let hx = high(x);
    let ix = hx & 0x7fffffff;
    if ix >= 0x3ff00000 {
        // |x| >= 1
        if ((ix - 0x3ff00000) | low(x) as i32) == 0 {
            // |x| is 1
            return if hx > 0 { 0.0 } else { PI + 2.0 * PIO2_LO };
        }
        return f64::NAN; // acos(|x| > 1) is NaN
    }
    if ix < 0x3fe00000 {
        // |x| < 0.5
        if ix <= 0x3c600000 {
            return PIO2_HI + PIO2_LO; // |x| < 2^-57
        }
        let z = x * x;
        let r = asin_p(z) / asin_q(z);
        return PIO2_HI - (x - (-x).mul_add(r, PIO2_LO));
    }
    if hx < 0 {
        // x < -0.5
        let z = (1.0 + x) * 0.5;
        let (pz, qz) = (asin_p(z), asin_q(z));
        let s = z.sqrt();
        let r = pz / qz;
        let w = r.mul_add(s, -PIO2_LO);
        return (-2.0f64).mul_add(s + w, PI);
    }
    // x > 0.5
    let z = (1.0 - x) * 0.5;
    let s = z.sqrt();
    let df = high_part(s);
    let c = (-df).mul_add(df, z) / (s + df);
    let r = asin_p(z) / asin_q(z);
    let w = r.mul_add(s, c);
    2.0 * (df + w)
}

/// Math.asin, as fdlibm's __ieee754_asin does.
pub(crate) fn asin(x: f64) -> f64 {
    const HUGE: f64 = 1.000e+300;
    const PIO4_HI: f64 = 7.85398163397448278999e-01;
    let hx = high(x);
    let ix = hx & 0x7fffffff;
    if ix >= 0x3ff00000 {
        // |x| >= 1
        if ((ix - 0x3ff00000) | low(x) as i32) == 0 {
            return x.mul_add(PIO2_HI, x * PIO2_LO); // asin(±1) = ±π/2
        }
        return f64::NAN; // asin(|x| > 1) is NaN
    }
    if ix < 0x3fe00000 {
        // |x| < 0.5
        let mut t = 0.0;
        if ix < 0x3e400000 {
            // |x| < 2^-27
            if HUGE + x > 1.0 {
                return x;
            }
        } else {
            t = x * x;
        }
        let w = asin_p(t) / asin_q(t);
        return x.mul_add(w, x);
    }
    // 1 > |x| >= 0.5
    let w = 1.0 - x.abs();
    let mut t = w * 0.5;
    let (pt, qt) = (asin_p(t), asin_q(t));
    let s = t.sqrt();
    if ix >= 0x3fef3333 {
        // |x| > 0.975
        let w = pt / qt;
        t = PIO2_HI - 2.0f64.mul_add(s.mul_add(w, s), -PIO2_LO);
    } else {
        let w = high_part(s);
        let c = (-w).mul_add(w, t) / (s + w);
        let r = pt / qt;
        let p = (2.0 * s).mul_add(r, -(-2.0f64).mul_add(c, PIO2_LO));
        let q = (-2.0f64).mul_add(w, PIO4_HI);
        t = PIO4_HI - (p - q);
    }
    if hx > 0 { t } else { -t }
}

/// Math.log10 as V8 works it out: fdlibm's e_log10.c and e_log.c, which give exact powers of
/// ten their whole logarithms.
pub(crate) fn log10(mut x: f64) -> f64 {
    const TWO54: f64 = 1.80143985094819840000e+16;
    const IVLN10: f64 = 4.34294481903251816668e-01;
    const LOG10_2HI: f64 = 3.01029995663611771306e-01;
    const LOG10_2LO: f64 = 3.69423907715893078616e-13;
    let (mut hx, mut lx) = (high(x), low(x));
    let mut k = 0;
    if hx < 0x00100000 {
        if ((hx & 0x7fffffff) | lx as i32) == 0 {
            return f64::NEG_INFINITY;
        }
        if hx < 0 {
            return f64::NAN;
        }
        k -= 54;
        x *= TWO54;
        (hx, lx) = (high(x), low(x));
    }
    if hx >= 0x7ff00000 {
        return x + x;
    }
    if hx == 0x3ff00000 && lx == 0 {
        return 0.0;
    }
    k += (hx >> 20) - 1023;
    let i = ((k as u32) >> 31) as i32;
    hx = (hx & 0x000fffff) | ((0x3ff - i) << 20);
    let y = f64::from(k + i);
    let x = words(hx, lx);
    let z = y.mul_add(LOG10_2LO, IVLN10 * log(x));
    y.mul_add(LOG10_2HI, z)
}

/// fdlibm's e_log.c, as V8 has it.
fn log(mut x: f64) -> f64 {
    const LN2_HI: f64 = 6.93147180369123816490e-01;
    const LN2_LO: f64 = 1.90821492927058770002e-10;
    const TWO54: f64 = 1.80143985094819840000e+16;
    const LG1: f64 = 6.666666666666735130e-01;
    const LG2: f64 = 3.999999999940941908e-01;
    const LG3: f64 = 2.857142874366239149e-01;
    const LG4: f64 = 2.222219843214978396e-01;
    const LG5: f64 = 1.818357216161805012e-01;
    const LG6: f64 = 1.531383769920937332e-01;
    const LG7: f64 = 1.479819860511658591e-01;
    let mut hx = high(x);
    let lx = low(x);
    let mut k = 0;
    if hx < 0x00100000 {
        if ((hx & 0x7fffffff) | lx as i32) == 0 {
            return f64::NEG_INFINITY;
        }
        if hx < 0 {
            return f64::NAN;
        }
        k -= 54;
        x *= TWO54;
        hx = high(x);
    }
    if hx >= 0x7ff00000 {
        return x + x;
    }
    k += (hx >> 20) - 1023;
    hx &= 0x000fffff;
    let i = (hx + 0x95f64) & 0x100000;
    let x = words(hx | (i ^ 0x3ff00000), low(x));
    k += i >> 20;
    let f = x - 1.0;
    if (0x000fffff & (2 + hx)) < 3 {
        if f == 0.0 {
            if k == 0 {
                return 0.0;
            }
            let dk = f64::from(k);
            return dk.mul_add(LN2_HI, dk * LN2_LO);
        }
        let r = (f * f) * (-0.33333333333333333f64).mul_add(f, 0.5);
        if k == 0 {
            return f - r;
        }
        let dk = f64::from(k);
        return dk.mul_add(LN2_HI, -((-dk).mul_add(LN2_LO, r) - f));
    }
    let s = f / (2.0 + f);
    let dk = f64::from(k);
    let z = s * s;
    let mut i = hx - 0x6147a;
    let w = z * z;
    let j = 0x6b851 - hx;
    let t1 = w * w.mul_add(w.mul_add(LG6, LG4), LG2);
    let t2 = z * w.mul_add(w.mul_add(w.mul_add(LG7, LG5), LG3), LG1);
    i |= j;
    let r = t2 + t1;
    if i > 0 {
        let hfsq = (0.5 * f) * f;
        if k == 0 {
            return f - (-s).mul_add(hfsq + r, hfsq);
        }
        return dk.mul_add(LN2_HI, -((hfsq - s.mul_add(hfsq + r, dk * LN2_LO)) - f));
    }
    if k == 0 {
        return (-s).mul_add(f - r, f);
    }
    dk.mul_add(LN2_HI, -(s.mul_add(f - r, -(dk * LN2_LO)) - f))
}
