#!/usr/bin/env python3
"""Coefficients for the saturator's 2x oversampling: a polyphase IIR half-band filter, two
chains of first-order allpasses (Valenzuela & Constantinides' design, as Laurent de Soras's HIIR
computes it). Prints them, and checks the filter's passband and stopband.

usage: scripts/halfband.py [coefficients] [transition bandwidth]"""

import cmath
import math
import sys


def transition_param(tbw):
    k = math.tan((1 - tbw * 2) * math.pi / 4) ** 2
    kksqrt = (1 - k * k) ** 0.25
    e = 0.5 * (1 - kksqrt) / (1 + kksqrt)
    e4 = e ** 4
    q = e * (1 + e4 * (2 + e4 * (15 + 150 * e4)))
    return k, q


def acc_num(q, order, c):
    acc, i, j = 0.0, 0, 1
    while True:
        t = q ** (i * (i + 1)) * math.sin((i * 2 + 1) * c * math.pi / order) * j
        acc += t
        j, i = -j, i + 1
        if abs(t) <= 1e-100:
            return acc


def acc_den(q, order, c):
    acc, i, j = 0.0, 1, -1
    while True:
        t = q ** (i * i) * math.cos(i * 2 * c * math.pi / order) * j
        acc += t
        j, i = -j, i + 1
        if abs(t) <= 1e-100:
            return acc


def coefs(n, tbw):
    k, q = transition_param(tbw)
    order = n * 2 + 1
    out = []
    for index in range(n):
        c = index + 1
        num = acc_num(q, order, c) * q ** 0.25
        den = acc_den(q, order, c) + 0.5
        ww = (num / den) ** 2
        x = math.sqrt((1 - ww * k) * (1 - ww / k)) / (1 + ww)
        out.append((1 - x) / (1 + x))
    return out


def response(cs, f):
    """|H| at f (cycles per sample of the 2x rate): H(z) = (A0(z^2) + z^-1 A1(z^2)) / 2."""
    z1 = cmath.exp(-2j * math.pi * f)
    z2 = z1 * z1
    def chain(cs):
        h = 1
        for a in cs:
            h *= (a + z2) / (1 + a * z2)
        return h
    return abs((chain(cs[0::2]) + z1 * chain(cs[1::2])) / 2)


if __name__ == "__main__":
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 8
    tbw = float(sys.argv[2]) if len(sys.argv) > 2 else 0.04
    cs = coefs(n, tbw)
    print("even (first chain):", ", ".join(f"{c:.12f}" for c in cs[0::2]))
    print("odd (second chain):", ", ".join(f"{c:.12f}" for c in cs[1::2]))
    db = lambda f: 20 * math.log10(max(response(cs, f), 1e-12))
    passband = [db(f / 1000) for f in range(0, int((0.25 - tbw) * 1000))]
    stopband = [db(f / 1000) for f in range(int((0.25 + tbw) * 1000) + 1, 500)]
    print(f"passband ripple {min(passband):.4f}..{max(passband):.4f} dB, stopband {max(stopband):.1f} dB")
    for f in (0.1, 0.2, 0.23, 0.25, 0.27, 0.3, 0.4):
        print(f"  {f:.2f}: {db(f):8.2f} dB")
