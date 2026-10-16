declare name "tempo Saturator";
declare description "Drive into a soft clip, as tape or a tube would: 2x oversampled and antialiased";

// A biased tanh, scaled so quiet sounds keep their level and loud ones round off; the bias adds
// the even harmonics tape and tubes are known for. It runs at twice the sample rate (a polyphase
// half-band filter: two allpass chains, from scripts/halfband.py 8 0.04, flat to 0.42 of the
// sample rate, 99 dB down past 0.58) with first-order antiderivative antialiasing (ADAA) on top,
// so even hard drive doesn't fold harmonics back down as aliasing. What it adds can be tilted
// darker or brighter, and mixed with the dry sound.

import("stdfaust.lib");

drive = hslider("drive", 6, 0, 24, 0.01) : si.smoo : ba.db2linear;
tone = hslider("tone", 0, -1, 1, 0.01) : si.smoo;
mix = hslider("mix", 1, 0, 1, 0.01) : si.smoo;
output = hslider("output", 0, -24, 24, 0.01) : si.smoo : ba.db2linear;

BIAS = 0.15;
TILT = 6; // dB each way
EVEN = (0.040633460924, 0.300757055992, 0.609524314896, 0.849223810392);
ODD = (0.150505129023, 0.460774504961, 0.738503841119, 0.949742783705);

// The half-band filter, H(z) = (A0(z^2) + z^-1 A1(z^2)) / 2, in its two phases: a sample in
// becomes the 2x samples 2n and 2n+1; two come back out as one.
allpass(a, x) = (\(y).(a * x + x' - a * y)) ~ _;
chain(cs) = seq(i, ba.count(cs), allpass(ba.take(i + 1, cs)));
up = _ <: chain(EVEN), chain(ODD);
down(e, o) = ((e : chain(EVEN)) + (o : chain(ODD))') / 2;

// ADAA: the curve's average between one 2x sample and the one before, from its antiderivative
// log cosh, so what the samples skip over isn't lost to aliasing.
// Each 2x sample's log cosh is worked out once and shared with the next sample's average.
logcosh(u) = abs(u) + log(1 + exp(-2 * abs(u))) - log(2);
average(u1, u0, l1, l0) = select2(near, (l1 - l0) / select2(near, d, 1), ma.tanh((u1 + u0) / 2))
with {
  d = u1 - u0;
  near = abs(d) < 0.001;
};
level(t) = (t - ma.tanh(BIAS)) / (drive * (1 - ma.tanh(BIAS) ^ 2));
shape(e, o) = level(average(ue, uo', le, lo')), level(average(uo, ue, lo, le))
with {
  ue = drive * e + BIAS;
  uo = drive * o + BIAS;
  le = logcosh(ue);
  lo = logcosh(uo);
};

tilt(x) = lp * ba.db2linear(-tone * TILT) + (x - lp) * ba.db2linear(tone * TILT)
with {
  lp = fi.lowpass(1, 1500, x);
};

blend(wet, dry) = (dry + (wet - dry) * mix) * output;
channel = up <: (shape : down : fi.dcblocker : tilt), down : blend;

process = channel, channel;
