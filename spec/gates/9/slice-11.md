# Gate 9 Slice 11 — Locked Dependencies, Package Data, Capabilities, and Cache Manifests

## Goal

Make non-source import inputs reproducible and capability-auditable without
claiming later wheel or native-extension compatibility.

## Integrated work

- Define the Rimera lock/manifest schema for canonical package names, versions,
  source hashes, module files, package data, declared native shells, targets,
  ABI versions, and capabilities.
- Embed/read declared package resources through the package/module owner and
  include every resource hash in graph/object/executable cache keys.
- Reject absent/stale locks, hash mismatches, undeclared modules/resources,
  unsupported native-extension entries, and denied capabilities before object
  or executable publication.
- Emit a deterministic build manifest listing graph order, objects, resources,
  hashes, runtime ABI, target, profile, and granted capabilities.

## Completion proof

Fixture projects prove identical manifests and artifacts from identical locked
inputs; source/resource/lock changes invalidate the right objects. Negative
capability/hash/native-entry cases produce stable diagnostics and no artifact.

## Closure evidence

`locked_package_resources_and_build_manifest_are_reproducible` proves identical
locked inputs produce deterministic manifests/artifacts and package resources
flow through the owning module. `stale_locked_resource_fails_before_artifact_publication`
proves stale resource hashes fail before publication. Both remain green in the
final workspace audit.

## DONE BY CHATGPT
