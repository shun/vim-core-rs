# Upstream test classification

This page explains how `vim-core-rs` classifies vendored upstream Vim
`src/testdir/test_*.vim` files and how it tracks repository-owned
adaptation coverage. It follows
`docs/adr/0002-define-compatibility-boundaries.md` and points to the
machine-readable manifest in `upstream-test-classification.json`.

## Summary

- Vendored upstream files: `311`
- In-scope compatibility denominator: `273`
- Preserve directly files: `231`
- Preserve through adaptation files: `42`
- Out of scope files: `38`
- Temporarily excluded files: `0`
- Adapted behaviors: `50`
- Covered adapted behaviors: `13`
- Uncovered adapted behaviors: `37`

The in-scope denominator is the upstream-derived embedded Vim core
baseline. It is computed as `231 preserve_directly + 42
preserve_through_adaptation = 273`, and it excludes the `38
out_of_scope` files.

## Manifest model

The classification manifest intentionally tracks two different units.

- `cases`: vendored upstream `test_*.vim` files and their generated-runner
  selection policy
- `adapted_behaviors`: repository-owned adaptation coverage units

This split exists because upstream Vim is tested as one integrated
binary, while `vim-core-rs` only owns the embedded core and its host
boundary contracts. Some upstream files map cleanly to one adapted
behavior. Others, such as `test_expand.vim`, mix multiple
responsibilities and have to be split into multiple adapted behaviors.

`Rendering State Family` phase 1 uses the same vocabulary across this
document, the scope page, and the contract tests. `Search` and `Syntax`
stay current members, `Annotations` stays a future placeholder for
text-property extraction, and `popupwin` remains host-owned presentation.
Issue #14 is the later facade/public-contract promotion step.

Within that Search family boundary, `incsearch` stays part of the
repository's search contract coverage. The contract keeps inactive window
queries and byte columns observable through `VimCoreSession`, while popup
ownership remains host-owned presentation instead of a crate rendering
contract. `search_capability_contract()` is the typed summary of that
boundary, so classification text and docs should agree with the public
contract fields instead of introducing a parallel vocabulary.

Some adapted behaviors also mix Vim-owned state with host-owned
presentation. For saya-like hosts that implement their own popup UI,
`popupwin` rendering stays host-owned and is not a crate extraction
contract. `highlight` is currently traceable through search highlight
ranges and syntax chunk extraction, but `:highlight` definition tables
and resolved attribute tables remain outside the public contract.
`textprop` remains Vim-owned annotation state and is deferred until the
repository defines a narrow read-only extraction surface.
Popup placement, popup composition, popup borders, and overlay layout also
remain host-owned presentation policy rather than crate extraction
contracts.

`build_test_runner.rs` consumes the manifest and emits a generated
manifest with:

- `compatibility_baseline` for file-level scope and generated-runner
  selection
- `adaptation_coverage` for behavior-level contract coverage

Generated upstream tests still cover only `preserve_directly` files.
Repository contract tests remain the source of truth for
`preserve_through_adaptation` behavior, including the repository-owned
contracts for VFS, VFD, session, events, runtime discovery, and other
host-boundary flows. `out_of_scope` files stay out of the generated
runner, and there are currently no `temporarily_excluded` files.
In the generated manifest, `adaptation_coverage.tracking_unit` is
`repo-owned adapted behavior` to make that behavior-level ownership
explicit.

Each `adapted_behaviors` entry carries machine-readable
`coverage_status` and optional `coverage_evidence`. Any behavior marked
`covered` must include `coverage_evidence` with a contract suite plus a
locator such as `test_name` or `evidence_ref`. The manifest validation
also requires the referenced contract suite to be declared in
`related_contract_suites`, and `test_name` locators must resolve to a
real repository contract test. When a rendering-state family member or
accessor is promoted, add the repository contract test before treating
that adapted behavior as covered.

## Runtime-path bucket

The runtime-path bucket now has behavior-level traceability in
repository contract tests and the generated manifest. The bucket
contains `16` adapted behaviors: `10` covered and `6` uncovered.

Covered runtime-path behaviors currently map to these contract tests:

- `runtimepath.autoload_source` from `test_autoload.vim`:
  `runtimepath_contract_supports_runtime_and_autoload_loading`
- `runtimepath.checkpath_includeexpr_recursion` from
  `test_checkpath.vim`:
  `runtimepath_contract_supports_checkpath_includeexpr_recursion`
- `runtimepath.expand.directory_wildcard_buffer_selection` from
  `test_expand.vim::Test_with_directories`:
  `runtimepath_contract_supports_wildcard_path_expansion_for_buffer_selection`
- `runtimepath.filetype_detection_from_runtime` from `test_filetype.vim`:
  `runtimepath_contract_supports_filetype_detection_from_runtime`
- `runtimepath.findfile_path_discovery` from `test_findfile.vim`,
  `runtimepath.fnameescape_path_quoting` from `test_fnameescape.vim`,
  `runtimepath.fnamemodify_path_transforms` from `test_fnamemodify.vim`,
  and `runtimepath.getcwd_working_directory_queries` from
  `test_getcwd.vim`:
  `runtimepath_contract_supports_path_discovery_and_fnameescape`
- `runtimepath.help_local_additions_from_runtime_docs` from
  `test_help.vim`:
  `runtimepath_contract_supports_help_local_additions_from_runtime_docs`
- `runtimepath.help_tagjump_from_runtime_docs` from
  `test_help_tagjump.vim`:
  `runtimepath_contract_supports_help_tagjump_from_runtime_docs`

The remaining runtime-path behaviors stay `uncovered` for now:

- `runtimepath.environ_home_and_environment_expansion` from
  `test_environ.vim`: no repository contract currently fixes the
  embedded environment mutation and `expand('~')` expectations
- `runtimepath.escaped_glob_and_globpath` from `test_escaped_glob.vim`:
  no repository contract currently fixes the escaped `glob()` or
  `globpath()` cases
- `runtimepath.expand_dllpath_options` from `test_expand_dllpath.vim`:
  no repository contract currently fixes the `*dll` option expansion
  behavior
- `runtimepath.expand_function_semantics` from `test_expand_func.vim`:
  no repository contract currently fixes the `expand()` function cases
  such as `<sfile>`, `<stack>`, or `'wildignore'`
- `runtimepath.glob2regpat_conversion` from `test_glob2regpat.vim`: no
  repository contract currently fixes the glob-to-regex conversion rules
- `runtimepath.global_command_path_sensitive_flows` from `test_global.vim`:
  no repository contract currently ties `:global` behavior to a
  runtime-path adaptation contract

`test_expand.vim` is no longer tracked as one indivisible runtime-path
case. The manifest now splits it into behavior entries, and only
`Test_with_directories` remains in the runtime-path bucket. The tilde,
environment, `expandcmd()`, and script-context functions from that file
now live in their own adaptation buckets.

## Classification rules

- **Preserve directly**: modal editing semantics and runtime behavior that
  `vim-core-rs` preserves as embedded Vim compatibility
- **Preserve through adaptation**: behavior that crosses the host
  boundary and is validated mainly through repository contract tests
- **Out of scope**: behavior outside the embedded-core product boundary,
  such as GUI, terminal-emulator parity, plugin hosting, clientserver,
  and external language interpreter integrations
- **Temporarily excluded**: files that are conceptually adjacent to the
  scope but are currently skipped for explicit repository reasons

## Operating rules

- Keep vendored upstream coverage in the generated runner only for
  `preserve_directly` files
- Track adapted coverage by repository-owned behavior, not by upstream
  file when a file mixes multiple responsibilities
- Reclassify cases when host-owned presentation and Vim-owned state need
  separate treatment, especially for popup windows, highlight state, and
  text properties
- Move host-boundary behavior into repository contract tests, even when
  upstream Vim has script coverage for similar scenarios
- Record policy exclusions in the classification manifest, not in the
  skiplist
- Reserve the skiplist for future `temporarily_excluded` files that
  still fit the repository boundary but cannot run yet for explicit
  reasons

## Next steps

- Split additional mixed upstream files into multiple adapted behaviors
  when their responsibilities cross buckets
- Refine per-behavior rationales as feature areas gain or lose scope
- Add or tighten repository contract tests when an adapted behavior
  gains new host-facing coverage
