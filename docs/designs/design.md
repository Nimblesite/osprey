The palette and screenshots are visual references. Use `website/src/` and the [messaging guide](../messaging.md) for current copy, runnable examples and supported behavior. Screenshots record earlier design studies.

![Colors](design.png)


---
name: Midnight Synthetic
colors:
  surface: '#0c1325'
  surface-dim: '#0c1325'
  surface-bright: '#33384c'
  surface-container-lowest: '#070d1f'
  surface-container-low: '#151b2d'
  surface-container: '#191f32'
  surface-container-high: '#23293d'
  surface-container-highest: '#2e3448'
  on-surface: '#dce1fb'
  on-surface-variant: '#bdc8cd'
  inverse-surface: '#dce1fb'
  inverse-on-surface: '#2a3043'
  outline: '#889297'
  outline-variant: '#3e484c'
  surface-tint: '#73d3f0'
  primary: '#bdeeff'
  on-primary: '#003642'
  primary-container: '#77d7f4'
  on-primary-container: '#005d70'
  inverse-primary: '#00677d'
  secondary: '#bbc5ec'
  on-secondary: '#252f4e'
  secondary-container: '#3e4868'
  on-secondary-container: '#adb7dd'
  tertiary: '#ffe1c0'
  on-tertiary: '#462b00'
  tertiary-container: '#ffbe65'
  on-tertiary-container: '#764c00'
  error: '#ffb4ab'
  on-error: '#690005'
  error-container: '#93000a'
  on-error-container: '#ffdad6'
  primary-fixed: '#b2ebff'
  primary-fixed-dim: '#73d3f0'
  on-primary-fixed: '#001f27'
  on-primary-fixed-variant: '#004e5e'
  secondary-fixed: '#dbe1ff'
  secondary-fixed-dim: '#bbc5ec'
  on-secondary-fixed: '#0f1a38'
  on-secondary-fixed-variant: '#3c4666'
  tertiary-fixed: '#ffddb5'
  tertiary-fixed-dim: '#fbba62'
  on-tertiary-fixed: '#2a1800'
  on-tertiary-fixed-variant: '#643f00'
  background: '#0c1325'
  on-background: '#dce1fb'
  surface-variant: '#2e3448'
typography:
  headline-lg:
    fontFamily: Geist
    fontSize: 40px
    fontWeight: '700'
    lineHeight: '1.2'
    letterSpacing: -0.02em
  headline-lg-mobile:
    fontFamily: Geist
    fontSize: 30px
    fontWeight: '700'
    lineHeight: '1.2'
  headline-md:
    fontFamily: Geist
    fontSize: 24px
    fontWeight: '600'
    lineHeight: '1.3'
  body-lg:
    fontFamily: Geist
    fontSize: 18px
    fontWeight: '400'
    lineHeight: '1.6'
  body-md:
    fontFamily: Geist
    fontSize: 16px
    fontWeight: '400'
    lineHeight: '1.6'
  code-sm:
    fontFamily: JetBrains Mono
    fontSize: 14px
    fontWeight: '400'
    lineHeight: '1.5'
  label-caps:
    fontFamily: JetBrains Mono
    fontSize: 12px
    fontWeight: '600'
    lineHeight: '1'
    letterSpacing: 0.05em
rounded:
  sm: 0.125rem
  DEFAULT: 0.25rem
  md: 0.375rem
  lg: 0.5rem
  xl: 0.75rem
  full: 9999px
spacing:
  base: 8px
  container-max: 1200px
  gutter: 24px
  margin-mobile: 16px
  margin-desktop: 48px
---

## Applying the palette

The [Osprey design system](../../website/design_system.md) defines typography, layout, components, motion, and video. The [website tokens](../../website/src/css/variables.css) implement this palette. Use those working values directly; avoid maintaining another set of component colors here.

Every website and video brief starts with [messaging](../messaging.md): effects first, real code, visible results, one next step.
