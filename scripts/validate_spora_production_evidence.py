#!/usr/bin/env python3
"""Validate Spora devnet production evidence before external release."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


EXPECTED_SCHEMA = "spora-devnet-production-evidence-v1"
EXPECTED_PROFILE = "production"
EXPECTED_STATUS = "passed"
EXPECTED_STANDARD_BLOCK_MAX_MASS = 2_000_000
EXPECTED_STANDARD_RELAY_MAX_TX_MASS = 500_000
EXPECTED_SCOPED_ACTION_COUNT = 43
EXPECTED_MALFORMED_ACTION_COUNT = 43
EXPECTED_BUNDLED_EXAMPLE_COUNT = 7

REQUIRED_CHECKS = [
    "production_gate_passed",
    "production_ready",
    "standard_mass_policy_used",
    "scoped_action_standard_relay_ready",
    "full_file_monolith_standard_relay_ready",
    "no_standard_relay_incompatible_examples",
]

REQUIRED_ARTIFACTS = [
    "acceptance_report",
    "base_report",
    "cellscript_report",
    "propagation_report",
    "sporad_log",
]


def load_json(path: Path) -> dict[str, Any]:
    try:
        with path.open("r", encoding="utf-8") as fh:
            value = json.load(fh)
    except FileNotFoundError as exc:
        raise SystemExit(f"missing evidence artifact: {path}") from exc
    except json.JSONDecodeError as exc:
        raise SystemExit(f"invalid JSON in {path}: {exc}") from exc
    if not isinstance(value, dict):
        raise SystemExit(f"{path} must contain a JSON object")
    return value


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"invalid Spora production evidence: {message}")


def require_field(mapping: dict[str, Any], key: str, expected: Any) -> None:
    actual = mapping.get(key)
    require(
        actual == expected,
        f"{key} must be {expected!r}, got {actual!r}",
    )


def resolve_artifact_path(raw: Any, evidence_dir: Path) -> Path:
    require(isinstance(raw, str) and raw, "artifact path must be a non-empty string")
    path = Path(raw)
    if not path.is_absolute():
        path = evidence_dir / path
    return path


def validate_required_artifacts(
    evidence: dict[str, Any],
    evidence_path: Path,
    require_artifact_files: bool,
) -> dict[str, Path]:
    artifacts = evidence.get("artifacts")
    require(isinstance(artifacts, dict), "artifacts must be an object")

    resolved: dict[str, Path] = {}
    for key in REQUIRED_ARTIFACTS:
        require(key in artifacts, f"artifacts.{key} is missing")
        path = resolve_artifact_path(artifacts[key], evidence_path.parent)
        if require_artifact_files:
            require(path.exists(), f"artifacts.{key} does not exist: {path}")
            require(path.stat().st_size > 0, f"artifacts.{key} is empty: {path}")
        resolved[key] = path
    return resolved


def validate_gate(gate: dict[str, Any]) -> None:
    require_field(gate, "status", EXPECTED_STATUS)
    require_field(gate, "production_ready", True)
    require_field(gate, "standard_mass_policy_used", True)
    require_field(gate, "standard_block_max_mass", EXPECTED_STANDARD_BLOCK_MAX_MASS)
    require_field(gate, "standard_relay_max_tx_mass", EXPECTED_STANDARD_RELAY_MAX_TX_MASS)
    require_field(gate, "scoped_action_artifact_count", EXPECTED_SCOPED_ACTION_COUNT)
    require_field(gate, "valid_action_specific_builder_count", EXPECTED_SCOPED_ACTION_COUNT)
    require_field(gate, "malformed_action_matrix_count", EXPECTED_MALFORMED_ACTION_COUNT)
    require_field(gate, "standard_relay_deploy_compatible_example_count", EXPECTED_BUNDLED_EXAMPLE_COUNT)
    require_field(gate, "bundled_example_count", EXPECTED_BUNDLED_EXAMPLE_COUNT)
    require_field(gate, "scoped_action_standard_relay_ready", True)
    require_field(gate, "full_file_monolith_standard_relay_ready", True)
    require_field(gate, "standard_relay_incompatible_examples", [])
    require_field(gate, "blockers", [])

    advisories = gate.get("advisories")
    require(isinstance(advisories, list), "production_gate.advisories must be a list")


def validate_against_base_report(evidence_gate: dict[str, Any], base_report_path: Path) -> None:
    base_report = load_json(base_report_path)
    base_gate = base_report.get("production_gate")
    require(isinstance(base_gate, dict), "base report is missing production_gate")

    compared_fields = [
        "status",
        "production_ready",
        "standard_mass_policy_used",
        "standard_block_max_mass",
        "standard_relay_max_tx_mass",
        "scoped_action_artifact_count",
        "valid_action_specific_builder_count",
        "malformed_action_matrix_count",
        "standard_relay_deploy_compatible_example_count",
        "bundled_example_count",
        "scoped_action_standard_relay_ready",
        "full_file_monolith_standard_relay_ready",
        "standard_relay_incompatible_examples",
        "blockers",
        "advisories",
    ]
    for field in compared_fields:
        require(
            evidence_gate.get(field) == base_gate.get(field),
            f"production_gate.{field} differs between evidence and base report",
        )

    require_field(base_gate, "required_action_specific_builder_count", EXPECTED_SCOPED_ACTION_COUNT)
    require_field(base_gate, "standard_relay_deploy_compatible_action_count", EXPECTED_SCOPED_ACTION_COUNT)
    require_field(base_gate, "scheduler_witness_shape_count", EXPECTED_SCOPED_ACTION_COUNT)
    require_field(base_gate, "scheduler_witness_shape_malformed_count", EXPECTED_SCOPED_ACTION_COUNT)
    require_field(base_gate, "bundled_example_deployment_probe_count", EXPECTED_BUNDLED_EXAMPLE_COUNT)


def validate_acceptance_report(acceptance_report_path: Path, evidence: dict[str, Any]) -> None:
    acceptance = load_json(acceptance_report_path)
    require_field(acceptance, "profile", EXPECTED_PROFILE)
    require_field(acceptance, "status", EXPECTED_STATUS)
    require_field(acceptance, "base", EXPECTED_STATUS)
    require_field(acceptance, "external_boot", EXPECTED_STATUS)
    require_field(acceptance, "cellscript", EXPECTED_STATUS)
    require_field(acceptance, "propagation", EXPECTED_STATUS)
    require(
        acceptance.get("git_revision") == evidence.get("git_revision"),
        "acceptance report git_revision differs from evidence",
    )


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Validate Spora production evidence emitted by scripts/devnet_acceptance.sh --profile production.",
    )
    parser.add_argument("evidence", type=Path, help="Path to production-evidence.json")
    parser.add_argument(
        "--skip-artifact-files",
        action="store_true",
        help="Validate the JSON envelope without requiring referenced artifacts to exist on this filesystem.",
    )
    parser.add_argument(
        "--require-clean-git",
        action="store_true",
        help="Fail if the evidence records git_dirty=true. Use this for tagged external releases.",
    )
    args = parser.parse_args()

    evidence_path = args.evidence.resolve()
    evidence = load_json(evidence_path)

    require_field(evidence, "schema", EXPECTED_SCHEMA)
    require_field(evidence, "profile", EXPECTED_PROFILE)
    require_field(evidence, "status", EXPECTED_STATUS)
    require(isinstance(evidence.get("run_id"), str) and evidence["run_id"], "run_id is missing")
    require(isinstance(evidence.get("git_revision"), str) and evidence["git_revision"], "git_revision is missing")
    if args.require_clean_git:
        require_field(evidence, "git_dirty", False)

    checks = evidence.get("required_checks")
    require(isinstance(checks, dict), "required_checks must be an object")
    for key in REQUIRED_CHECKS:
        require_field(checks, key, True)

    gate = evidence.get("production_gate")
    require(isinstance(gate, dict), "production_gate must be an object")
    validate_gate(gate)

    artifacts = validate_required_artifacts(
        evidence,
        evidence_path,
        require_artifact_files=not args.skip_artifact_files,
    )
    if not args.skip_artifact_files:
        validate_acceptance_report(artifacts["acceptance_report"], evidence)
        validate_against_base_report(gate, artifacts["base_report"])

    print(f"valid Spora production evidence: {evidence_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
