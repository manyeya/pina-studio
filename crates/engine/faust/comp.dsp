declare name "Pina Studio Compressor";
declare description "A soft-knee feed-forward compressor, both sides linked, peak or RMS";

// Faust's compressor gain computer (co.peak_compression_gain_N_chan_db): a 6 dB soft knee, the
// gain smoothed in dB by the attack and release, both sides turned down by the louder. Its level
// is the signal's peak, or an RMS average over 15 ms. A third output is the gain reduction, in
// dB, for the meter.

import("stdfaust.lib");

threshold = hslider("threshold", -18, -60, 0, 0.01);
ratio = hslider("ratio", 4, 1, 20, 0.01);
attack = hslider("attack", 10, 0.05, 1000, 0.01) * 0.001;
release = hslider("release", 120, 1, 5000, 0.01) * 0.001;
makeup = hslider("makeup", 0, -24, 24, 0.01) : si.smoo;
rms = checkbox("rms");

KNEE = 6;
strength = 1 - 1 / max(1, ratio);

level = _ <: _, an.rms_envelope_tau(0.015) : select2(rms);
gains = par(i, 2, level) : co.peak_compression_gain_N_chan_db(strength, threshold, attack, release, KNEE, 1, 1, 2);
apply(l, r, gl, gr) = l * ba.db2linear(gl + makeup), r * ba.db2linear(gr + makeup), 0 - min(gl, gr);

process = si.bus(2) <: si.bus(2), gains : apply;
