# Synthesis research [SYNTH-RESEARCH]

Research gathered on 2026-11-14 for the independent `@osprey/synth` toolkit. Light Orchestra and Prism are consumers, not the architecture. The [library contract](../../specs/runtime/Synthesis.md) separates delivered functionality from proposed capabilities. This is a curated starting collection, not a systematic review or a claim of novelty.

Entries link to primary papers, author publications, institutional records or official engineering documentation. Summaries describe the cited work; **Osprey experiments** are our proposed applications, not claims made by those authors. Publication dates below come from the work or its bibliographic record, rather than search-engine crawl dates. Full papers and third-party implementations retain their own licences; this collection contains links and brief annotations, not redistributed paper PDFs.

## Reading path [SYNTH-RESEARCH-PATH]

Start with Puckette for signals and synthesis, Chowning for modulation, Välimäki/Pekonen/Nam for oscillator aliasing, and Zavalishin for stateful filters. Then study waveshaping antialiasing, spectral and physical models. Read DDSP alongside its review and newer parameter-discovery work to distinguish useful learnable controls from expensive waveform generation. Keep real-time callback constraints beside every algorithm.

The architectural opportunity is a common graph and parameter language that supports hand-designed, physically modelled and learned instruments, with interchangeable offline/reference and real-time renderers. This is a design hypothesis to test, not an established research breakthrough.

## Signals, spectra and foundational synthesis [SYNTH-RESEARCH-FOUNDATIONS]

### Puckette — The Theory and Technique of Electronic Music (2007)

[Author's online book, HTML and PDF](https://msp.ucsd.edu/techniques.htm). A foundation for digital oscillators, amplitude control, modulation, delay-based processing and Fourier techniques. Use the author's worked material to establish units, phase conventions and analytical reference cases before optimising kernels. **Osprey experiment:** render a fixed patch in one block and many irregular blocks; compare samples and final state. This is a book, not a new synthesis paper.

### Chowning — The Synthesis of Complex Audio Spectra by Means of Frequency Modulation (1973)

[Journal of the Audio Engineering Society 21(7), 526–534](https://aes.org/publications/elibrary/elibrary-page/?id=1954). Demonstrates economical control over evolving spectra through modulation. **Osprey experiment:** a composable FM/phase-modulation graph with explicit modulation units; measure sidebands and aliasing as index and carrier/modulator ratio change. Publisher bibliographic record and abstract verified; full-text access may depend on access rights.

### Moorer — The Synthesis of Complex Audio Spectra by Means of Discrete Summation Formulas (1976)

[JAES 24(9), 717–727](https://aes.org/publications/elibrary/elibrary-page/?id=2590). Discrete summation gives economical spectral construction with explicit partial-count control. **Osprey experiment:** compare bounded additive banks with closed-form evaluation across near-singular parameters and Nyquist boundaries. Bibliographic record/abstract verified; obtain the full derivation before implementing it.

### Serra and Smith — Spectral Modeling Synthesis (1990)

[Author institution's paper record](https://mtg.upf.edu/node/251) and [MTG's publications and tools](https://www.upf.edu/web/mtg/sms-tools). Models a sound through time-varying sinusoidal components plus a stochastic residual. **Osprey experiment:** additive partial banks and filtered noise as independently routable nodes; preserve phase continuity while changing pitch and spectral envelopes. Transient handling and analysis errors need separate evaluation.

## Oscillators, filters and nonlinearities [SYNTH-RESEARCH-ANTIALIASING]

### Välimäki, Pekonen and Nam — Perceptually informed synthesis of bandlimited classical waveforms using integrated polynomial interpolation (2012)

[Institutional record, JASA 131(1), 974–986](https://research.aalto.fi/fi/publications/perceptually-informed-synthesis-of-bandlimited-classical-waveform/). A key source for polynomial-based alias reduction in classical waveforms. Read alongside [Pekonen's doctoral thesis, Filter-Based Oscillator Algorithms for Virtual Analog Synthesis](https://research.aalto.fi/en/publications/filter-based-oscillator-algorithms-for-virtual-analog-synthesis/), which describes bandlimited impulse/step approaches and their approximations. **Osprey experiment:** compare naive, polynomial-corrected and high-rate reference oscillators under static pitch, PWM, hard sync and modulation; report residual alias energy and CPU separately. A polynomial correction is not a universal alias-free guarantee.

### Zavalishin — The Art of VA Filter Design, revision 2.1.2 (2020)

[Author's book hosted by Native Instruments](https://www.native-instruments.com/fileadmin/ni_media/downloads/pdf/VAFilterDesign_2.1.2.pdf). Reference for topology-preserving transforms, integrators and feedback in virtual-analogue filters. **Osprey experiment:** an actual stateful filter with explicit integrator state, checked cutoff/resonance, modulation trajectories and impulse/step tests. Prism's existing scalar attenuation at the fundamental is not a stateful low-pass filter and cannot substitute for one.

### Parker, Zavalishin and Le Bivic — Reducing the aliasing of nonlinear waveshaping using continuous-time convolution (DAFx 2016)

[Conference paper, PDF](https://www.dafx.de/paper-archive/2016/dafxpapers/20-DAFx-16_paper_41-PN.pdf). Develops antiderivative-based treatment of nonlinear waveshaping. **Osprey experiment:** compare direct waveshaping, oversampling and ADAA on a common input, with matched level and latency; test nearly equal adjacent input values and transitions through zero. Do not assume a memoryless technique transfers unchanged into a feedback loop.

### Holters — Combined Derivative/Antiderivative Antialiasing (DAFx 2022)

[Conference paper, PDF](https://www.dafx.de/paper-archive/2022/papers/DAFx20in22_paper_7.pdf). Investigates derivative samples combined with ADAA. The abstract explicitly reports that the method compares unfavourably with oversampled ADAA. **Osprey experiment:** retain negative-result baselines; compare quality per cost rather than selecting an algorithm because it is newer.

### Smart — Wave Pulse Phase Modulation: Hybridising Phase Modulation and Phase Distortion (DAFx 2025)

[Conference paper, PDF](https://www.dafx.de/paper-archive/2025/DAFx25_paper_48.pdf). Introduces WPPM, combining phase-shaping approaches into a synthesis technique. **Osprey experiment:** represent phase transformation as a component, audition its range and stress its discontinuities against oversampled references. Include it as a research candidate after basic phase/modulation contracts are stable; publication does not establish our implementation's real-time or aliasing performance.

## Grains and physical models [SYNTH-RESEARCH-MODELS]

### Truax — Granular Synthesis (author's overview)

[Simon Fraser University author page](https://www.sfu.ca/~truax/gran.html). Explains dense collections of short grains and the author's real-time implementation history. **Osprey experiment:** fixed-capacity grain pools with explicit windows, seeded randomness, pitch/time controls and a defined overload policy. Validate window sums and density-dependent level, not just whether a cloud sounds interesting. This source is an overview, not a newly published paper.

### Roads — From grains to forms (author-hosted article)

[UCSB author PDF](https://clang.mat.ucsb.edu/clang/articles_files/From%20grains%20to%20forms.pdf). Connects granular techniques with larger musical structures. **Osprey experiment:** expose grain distributions and reproducible event schedules as composable control signals rather than one opaque texture preset. Exact publication metadata should be taken from the full text before a formal bibliography citation; the author-hosted document is the stable reading link.

### Smith — A Basic Introduction to Digital Waveguide Synthesis

[Author's paper hosted in university course materials](https://courses.cs.washington.edu/courses/cse467/05wi/pdfs/swgt.pdf). Explains digital-waveguide modelling of musical systems. **Osprey experiment:** fractional delay, loss filters, excitation and resonators as separate components, with measured tuning and decay. The broader [Physical Audio Signal Processing book](https://ccrma.stanford.edu/~jos/pasp/) is supplementary reading; the Stanford page blocked automated retrieval during this collection, so its contents were not freshly verified.

### Bilbao — Numerical Sound Synthesis: Finite Difference Schemes and Simulation in Musical Acoustics (2009)

[University bibliographic record](https://www.research.ed.ac.uk/en/publications/numerical-sound-synthesis-finite-difference-schemes-and-simulatio/) and [author's companion page](https://www2.ph.ed.ac.uk/~sbilbao/nss.html). A physical-modelling foundation distinct from simple oscillator emulation. **Osprey experiment:** modal and finite-difference resonators with stated discretisation limits, energy/decay checks and resource budgets. Stability restrictions belong in preparation-time validation, not an undocumented preset convention.

## Differentiable and learned synthesis [SYNTH-RESEARCH-LEARNED]

### Engel, Hantrakul, Gu and Roberts — DDSP: Differentiable Digital Signal Processing (ICLR 2020)

[Paper](https://arxiv.org/abs/2001.04643) and [author project explanation](https://magenta.tensorflow.org/ddsp). Combines interpretable DSP elements with learned controls. **Osprey experiment:** keep synthesis operators and parameter trajectories explicit, with an offline differentiable counterpart and parity tests against the production renderer. A differentiable model is not automatically causal, low latency or safe for an audio callback.

### Shan, Hantrakul, Chen, Avent and Trevelyan — Differentiable Wavetable Synthesis (2021 preprint)

[Paper](https://arxiv.org/abs/2111.10003) and [author audio supplement](https://lamtharnhantrakul.github.io/diffwts.github.io/). Learns a compact dictionary of periodic waveforms. **Osprey experiment:** offline table learning and validation, immutable playback assets, spectral mip levels and reproducible table morphing. Learning a waveform does not remove interpolation or pitch-dependent aliasing requirements.

### Caillon and Esling — RAVE: A variational autoencoder for fast and high-quality neural audio synthesis (2021 preprint)

[Paper](https://arxiv.org/abs/2111.05011) and [IRCAM project](https://acids-ircam.github.io/RAVE/). Uses a learned latent representation and multiband waveform processing for efficient neural synthesis. **Osprey experiment:** a separately benchmarked neural node with explicit history, latency, memory and supported block sizes. The authors' reported speed on their setup is not an Osprey performance result; throughput alone does not establish interactive latency.

### Hayes, Shier, Fazekas, McPherson and Saitis — A review of differentiable digital signal processing for music and speech synthesis (2024)

[Author-institution PDF](https://comma.eecs.qmul.ac.uk/assets/pdf/Hayes_DDSP_Review.pdf), DOI `10.3389/frsip.2023.1284100`; [2023 preprint](https://arxiv.org/abs/2308.15422). Surveys operators, applications and optimisation/robustness difficulties. **Osprey experiment:** use this to choose tractable targets and test gradient behaviour, parameter identifiability and out-of-distribution input, not just reconstruction on training examples.

### Yutani, Yamamoto, Nakatani and Terasawa — Wavetable Synthesis Using CVAE for Timbre Control Based on Semantic Label (APSIPA ASC 2024)

[Paper/preprint](https://arxiv.org/abs/2410.18628). Connects semantic timbre labels with wavetable generation through a conditional VAE. **Osprey experiment:** optional semantic control that produces ordinary inspectable parameters/assets; retain manual control and evaluate ambiguity across listeners and instruments. It belongs outside the required sample-rendering path.

### Mitcheltree, Tan and Reiss — Modulation Discovery with Differentiable Digital Signal Processing (WASPAA 2025)

[Paper/preprint](https://arxiv.org/abs/2510.06204). Estimates structured modulation rather than only opaque framewise controls; the authors provide audio, code and plug-ins. **Osprey experiment:** infer editable envelopes/LFO routings from sound, then render them through the same deterministic graph used for hand-authored patches. Measure the trade-off between interpretable controls and matching accuracy.

### Takemoto, Nakamura and Saruwatari — DDSP Mixture Model-Guided Diffusion for Synthesis Parameter Estimation from Harmonic Sound Mixtures (2026)

[September 2026 preprint](https://arxiv.org/abs/2609.14427), whose record reports acceptance at APSIPA ASC 2026. Adds temporal plausibility to synthesis-parameter estimation from mixtures through diffusion-guided estimation. **Osprey experiment:** offline extraction of editable ensemble parts. This is a recent analysis/estimation candidate, not evidence that diffusion belongs in a low-latency instrument callback. Full independent reproduction is pending.

## Engineering references [SYNTH-RESEARCH-ENGINEERING]

[PortAudio's callback guidance](https://portaudio.com/docs/v19-doxydocs/writing_a_callback.html) identifies allocation, I/O, mutexes and other unbounded operations as callback hazards. This informs the real-time boundary even if a platform uses another audio API. Preparing graphs and publishing Signals must not introduce these operations into the renderer.

[Steinberg's official VST3 SDK](https://github.com/steinbergmedia/vst3sdk) is the future **hosting** reference: loading existing plug-ins, audio/event buses, parameter automation, state and latency. [CLAP's official API](https://github.com/free-audio/clap) is a useful comparison for plug-in boundary design. Neither adapter is implemented by collecting these references, and VST3 hosting is explicitly deferred from the first library extraction.

## Research-to-delivery experiments [SYNTH-RESEARCH-EXPERIMENTS]

| Track | Candidate deliverable | Evidence required before adoption |
| --- | --- | --- |
| Oscillators | Corrected periodic sources, tables, modulation and sync | Frequency/phase tests, pitch sweeps, coherent spectral measurements against a higher-quality reference, alias/CPU trade-off |
| Stateful DSP | Envelopes, smoothing, filters and delay | Exact state continuity across block splits; impulse/step responses; reset, retrigger, rate-change and extreme-parameter behaviour |
| Nonlinear DSP | ADAA/oversampled waveshaping and feedback models | Level/latency-matched comparisons, near-equal inputs, bounded solver iterations, finite output and measured residual aliasing |
| Graph engine | Typed rates/channels/ports, explicit feedback, prepared schedule | Invalid-route rejection, deterministic event order, buffer lifetime proof/checks, allocation and lock instrumentation |
| Physical/granular | Resonators, excitation and grain clouds | Stable energy/decay, tuning, seeded replay, fixed capacity and overload policy |
| Learned control | Parameter fitting, learned tables and interpretable modulation | Held-out evaluation, manual editability, model/data provenance and offline/production parity |
| Neural audio | Optional learned waveform node | Actual target-device callback deadlines, latency, memory and degradation behaviour |
| Signals/UI bridge | Observable parameters, voices, meters and diagnostics | Consistent revisions, bounded publication, no callback blocking, detached/slow subscriber tests |
| VST3 hosting | Third-party plug-in graph nodes | Host lifecycle tests, sample-offset events, latency compensation, state restoration and process-failure handling |

For each experiment retain its source revision, dataset/patch inputs, sample rate, block schedule, compiler flags, hardware, audio output and result tables. Pair objective measurements with level-matched listening comparisons. An attractive demo or a paper's headline result is insufficient to label a component production-ready.
