declare name "tempo Channel EQ";
declare description "A low cut, two bells and a high shelf, as Logic's Channel EQ";

// Andy Simper's state-variable filters (fi.svf), whose responses are the RBJ cookbook's: the
// curve tempo draws (dsp::eq_response) is what's heard. Frequencies and gains are smoothed, so
// moving them doesn't click; the low cut fades in and out.

import("stdfaust.lib");

freq(f) = f : max(10) : min(ma.SR * 0.45) : si.smoo;
gain = si.smoo;

low_cut = hslider("low_cut", 0, 0, 20000, 0.01);
low_mid_freq = hslider("low_mid_freq", 400, 10, 20000, 0.01);
low_mid_gain = hslider("low_mid_gain", 0, -30, 30, 0.01);
low_mid_q = hslider("low_mid_q", 1, 0.1, 18, 0.001);
high_mid_freq = hslider("high_mid_freq", 2500, 10, 20000, 0.01);
high_mid_gain = hslider("high_mid_gain", 0, -30, 30, 0.01);
high_mid_q = hslider("high_mid_q", 1, 0.1, 18, 0.001);
shelf_freq = hslider("shelf_freq", 10000, 10, 20000, 0.01);
shelf_gain = hslider("shelf_gain", 0, -30, 30, 0.01);
shelf_q = hslider("shelf_q", 0.7071, 0.1, 18, 0.001);

// At or under 20 Hz the low cut is off.
cut_on = low_cut > 20 : si.smoo;
cut(x) = x + (fi.svf.hp(freq(low_cut), 0.7071067811865476, x) - x) * cut_on;

channel = cut
  : fi.svf.bell(freq(low_mid_freq), low_mid_q, gain(low_mid_gain))
  : fi.svf.bell(freq(high_mid_freq), high_mid_q, gain(high_mid_gain))
  : fi.svf.hs(freq(shelf_freq), shelf_q, gain(shelf_gain));

process = channel, channel;
