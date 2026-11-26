# Synthesis research [SYNTH-RESEARCH]

The [2026-11-17 focused collection](#neural-granular-and-physical-synthesis-synth-research-focused) supports the [proposed package and Prism contracts](../../specs/synth/SynthAdvanced.md#advanced-synthesis-proposal-synth-advanced) and [delivery plan](../../plans/synth/prism-advanced.md#advanced-synthesis-delivery-plan-prism-advanced-plan).

Research gathered on 2026-11-14 for the independent `@osprey/synth` toolkit. Light Orchestra and Prism are consumers, not the architecture. The [library contract](../../specs/synth/Synthesis.md) separates delivered functionality from proposed capabilities. This is a curated starting collection, not a systematic review or a claim of novelty.

Entries link to primary papers, author publications, institutional records or official engineering documentation. Summaries describe the cited work; **Osprey experiments** are our proposed applications, not claims made by those authors. Publication dates below come from the work or its bibliographic record, rather than search-engine crawl dates. Full papers and third-party implementations retain their own licences; this collection contains links and brief annotations, not redistributed paper PDFs.

## Reading path [SYNTH-RESEARCH-PATH]

Start with Puckette for signals and synthesis, Chowning for modulation, Välimäki/Pekonen/Nam for oscillator aliasing, and Zavalishin for stateful filters. Then study waveshaping antialiasing, spectral and physical models. Read DDSP alongside its review and newer parameter-discovery work to distinguish useful learnable controls from expensive waveform generation. Keep real-time callback constraints beside every algorithm.

For morphing between two analysed sounds, the [spectral morphing note](spectral-morphing.md) compares the literature's methods and justifies the Blend, Envelope and Partial modes [SYNTH-SPECTRAL-MORPH].

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

## Neural, granular and physical synthesis [SYNTH-RESEARCH-FOCUSED]

Collected 2026-11-17. The following annotations describe primary publications and manufacturer documentation. Proposed applications are design decisions for Osprey. Product descriptions establish documented behavior, with listening, source-level reproduction and target-device benchmarks remaining future work. Source versions are identified where available.

### Neural models and deployed tools [SYNTH-RESEARCH-NEURAL]

| Source | Technical contribution and consequence for the design |
| --- | --- |
| Caillon and Esling, *RAVE* (2021 preprint) | A variational autoencoder uses a compact latent representation, multiband synthesis and two-stage training. Expose model-specific latent controls and measure buffering, lookahead and processing separately. The paper's throughput result does not establish Prism latency. [Paper](https://arxiv.org/abs/2111.05011) |
| Engel et al., *DDSP* (ICLR 2020) | Differentiable signal-processing operators let learned models control structured synthesis. Pitch/loudness-conditioned harmonic and noise resynthesis suggests an editable timbre-transfer module. Pitch-estimation errors and polyphonic inputs need explicit evaluation. [Paper](https://arxiv.org/abs/2001.04643) |
| Défossez et al., *High Fidelity Neural Audio Compression* (2022 preprint) | EnCodec learns an encoder/decoder and quantized audio representation. Codec tokens require model-specific metadata and compatible codebooks. Reconstructing compressed audio alone supplies no text conditioning or interpretable timbre axis. [Paper](https://arxiv.org/abs/2210.13438), [implementation](https://github.com/facebookresearch/encodec) |
| Liu et al., *AudioLDM* (ICML 2023) | Text-conditioned latent diffusion generates audio using CLAP representations. Use an asynchronous asset-generation job with seed, model revision and output provenance, followed by ordinary sample loading. [Paper](https://arxiv.org/abs/2301.12503), [proceedings](https://proceedings.mlr.press/v202/liu23f.html) |
| Magenta DDSP-VST | Provides neural timbre-transfer effects and a MIDI-playable synthesizer. This motivates separate Audio and MIDI input modes, with visible pitch tracking and level control. [Developer repository](https://github.com/magenta/ddsp-vst), [official plugin catalog](https://magenta.withgoogle.com/plugins) |
| IRCAM `nn~` | Hosts exported neural models in Max and Pure Data, including RAVE models. Model methods and channel dimensions form an integration contract. Prism must obtain these from a validated manifest. [Developer repository](https://github.com/acids-ircam/nn_tilde) |
| Neutone FX | The published catalog includes DDSP instrument models and RAVE timbre-transfer models with declared input/output layouts. A model browser should expose those capabilities and provenance before insertion. [Official model catalog](https://neutone.jp/fx/models) |

### Granular instruments [SYNTH-RESEARCH-GRANULAR-PRODUCTS]

Truax's author overview describes dense clouds of short events and links his *Real-Time Granular Synthesis with a Digital Signal Processor* (Computer Music Journal 12(2), 1988). This supplies the scheduling/windowing foundation. The proposed 2–500 ms control range is a musical design choice extending beyond the overview's typical short-grain examples. [Author overview and bibliography](https://www.sfu.ca/~truax/gran.html)

| Instrument and inspected source | Documented approach | Proposed Prism application |
| --- | --- | --- |
| Pigments, manual 5.0 | Granulation is part of the Sample engine. The indexed official manual establishes that relationship. Full PDF retrieval exceeded the browser limit, so detailed control parity remains unverified. | A sample waveform with position, grain size and density in the selected module inspector. [Official manual](https://dl.arturia.net/products/pigments/manual/pigments_Manual_5_0_0_EN.pdf) |
| Portal, Grain Controls help page | Captures incoming audio into a buffer, with grain density, length, count, pitch, reverse, offset, shape and feedback. Timing controls can use rhythmic relationships. | An explicit live-input mode with capture/freeze, tempo synchronization and reproducible randomness. [Manufacturer technical guide](https://support.output.com/en/articles/10297851-grain-controls) |
| Quanta 2, manufacturer product documentation | A sample-based grain voice combines grain rate, pitch, direction, shape, length and position with synthesis sources and modulation. | Per-note grain voices and modulation destinations with visible resource limits. [Manufacturer documentation](https://www.audiodamage.com/collections/granular-synthesis/products/ad055-quanta-2) |
| Tasty Chips GR-1, manual 3.4.b | Hardware granular instrument organized around samples, scan position and direct grain controls. | Waveform scanning, frozen-position performance and direct numeric control alongside gestures. [Manufacturer manual](https://www.tastychips.nl/downloads/GR-1/UserManual/User-manual-GR-1-v3.4.b.pdf) |
| Make Noise Morphagene, manufacturer manual | Organizes material into Reels, Splices and Genes. Gene Size, Slide, Morph and Vari-Speed control microsound playback. Recording and playback can operate independently. | Region selection and grain scanning with independently specified capture, transport and pitch behavior. [Manufacturer manual](https://www.makenoisemusic.com/wp-content/uploads/2024/03/morphagene-manual.pdf) |

### Physical instruments and numerical methods [SYNTH-RESEARCH-PHYSICAL-PRODUCTS]

Bilbao et al., *Physical modeling, algorithms and sound synthesis — The NESS Project*, Computer Music Journal 43(2–3), 15–30, is recorded by Edinburgh as published in 2020. It discusses numerical simulation and the relationship between physical systems and synthesis algorithms. Use energy behavior and numerical stability as design obligations for strings, tubes, membranes and coupled bodies. The source's publication record takes precedence over search crawl dates. [Institutional full text](https://www.pure.ed.ac.uk/ws/portalfiles/portal/121715237/cmj_ness_aam.pdf)

| Instrument | Documented approach | Proposed Prism application |
| --- | --- | --- |
| Mutable Instruments Rings | Modal resonator, sympathetic strings and a dispersive string model, with excitation supplied externally or internally. | Independently routable excitation and body, plus compact material, brightness, damping and position controls. [Original-author manual](https://pichenettes.github.io/mutable-instruments-documentation/modules/rings/manual/) |
| Mutable Instruments Elements | Modal voice with blow, bow and strike exciters, geometry and material controls. | Excitation choices with physically meaningful controls and visible body response. [Original-author manual](https://pichenettes.github.io/mutable-instruments-documentation/modules/elements/manual/) |
| Rings/Elements clones | The original documentation supplies a reference design. Clone hardware and alternate firmware may change modes, controls and behavior. | Record exact firmware revision and hardware before parity testing. No clone-wide compatibility claim follows from the original manual. [Original source repository](https://github.com/pichenettes/eurorack) |
| AAS Chromaphone 3 | Couples resonant objects so they exchange energy, alongside excitation and resonator controls. | Coupling belongs inside a bounded coupled-body processor with a documented energy model. [Manufacturer manual, Coupling of Resonators](https://www.applied-acoustics.com/chromaphone-3/manual/) |
| Ableton Collision, Live 12 manual | Mallet/noise excitation and resonator sections expose the construction of a physical percussion voice. | A strike-to-body workflow and material presets with editable parameters. [Official instrument reference](https://www.ableton.com/en/live-manual/12/live-instrument-reference/#collision) |
| Ableton Tension, Live 12 manual | Models string excitation, damping, termination, pickup and body. Exciters include bow, hammer, bouncing hammer and plectrum. | Separate playing gesture from string/body properties, with conditional controls for each exciter. [Official instrument reference](https://www.ableton.com/en/live-manual/12/live-instrument-reference/#tension) |

The earlier Washington-hosted Smith paper now redirects to authentication, and Stanford's book blocks automated retrieval. Keep those bibliographic leads with that access limitation. The accessible NESS paper and original instrument manuals support this proposal's physical-model requirements.

### Combining the methods and assessing originality [SYNTH-RESEARCH-HYBRID]

| Prior art | Consequence for the proposed combined method |
| --- | --- |
| *Neural Granular Sound Synthesis* (2020 preprint, revised 2021) | Learned grain representations and trajectories already exist. A neural grain generator alone is established prior art. [Paper](https://arxiv.org/abs/2008.01393) |
| Castle, Adcock and Barrass, *Integrated Modal and Granular Synthesis of Haptic Tapping and Scratching Sounds* (Eurohaptics 2002) | Combines granular scratching and modal tapping for interactive objects. The author publication page and indexed paper abstract establish this precedent. Full mirrored PDF retrieval failed. [Author publication entry](https://sonification.com/2002/10/10/haptic-scratching-and-tapping/), [paper mirror](https://citeseerx.ist.psu.edu/document?doi=8bb69ce847735352895718172f138fae43e3a5f2&repid=rep1&type=pdf) |
| *Differentiable Modal Synthesis for Physical Modeling of Planar String Sound and Motion Simulation* (NeurIPS 2024) | Differentiable physical modal models already connect learned control and instrument physics. [Conference publication](https://papers.nips.cc/paper_files/paper/2024/hash/0232cafe8d1909a01019abe8af32f3e1-Abstract-Conference.html), [preprint](https://arxiv.org/abs/2407.05516) |
| *Stable Differentiable Modal Synthesis for Learning Nonlinear Dynamics* (2026) | Stability-aware learned modal dynamics are also prior art. Evaluate its assumptions before selecting a stability parameterization. [Paper](https://arxiv.org/abs/2601.10453), [authors' implementation](https://github.com/victorzheleznov/jaes-modal-node) |
| sonicLAB SSNN | The manufacturer describes a spiking neural network driving multiple engines including granular and modal engines. Shared neural event triggering is already a product design. [Manufacturer description](https://www.sonic-lab.com/ssnn-page) |

Our proposed experiment gives each grain a contact position, duration, force budget and latent descriptor within one evolving physical body. The body's previous modal state influences subsequent grain selection and timing. A learned controller proposes these interactions, and a checked physical update constrains them. The potential contribution is this particular state-dependent interaction and its controllable behavior. Originality remains a hypothesis after this bounded search. The [combined-method contract](../../specs/synth/SynthAdvanced.md#combined-grain-and-body-method-synth-advanced-hybrid) defines the algorithm and comparisons needed to evaluate it.
