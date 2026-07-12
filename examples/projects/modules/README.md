# Mixed-flavour modules

This project exercises Osprey's logical module graph end to end:

- `"billing/api"::Tax` is written in Default Osprey under the intentionally
  unrelated physical path `src/deep/unrelated/tax.osp`; the slash label is one
  opaque namespace name, not a folder hierarchy.
- `billing::Greeting` and the signature-ascribed `billing::Counter` state module
  are written in the ML flavour; `Greeting` aliases the quoted namespace and
  imports Default's `Tax::addTax` through `taxApi::Tax::addTax`.
- the Default entry imports both ML modules, proving interop in both directions
  through one shared ABI and namespace model.
- `Counter` state is instantiated by `run` and touched only by `CounterFx`
  handler arms. Importing it allocates and mutates nothing.
- `CounterFake` handles the same exported effect without owning mutable state,
  so tests can swap the installer while application logic stays unchanged.

Run it from the repository root:

```sh
target/release/osprey build examples/projects/modules -o /tmp/osprey-modules
/tmp/osprey-modules
```
