#!/usr/bin/env bash
set -Eeuo pipefail

PROFILE="base"
KEEP_ARTIFACTS=0
ACCEPTANCE_COMMAND="$0 $*"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --profile)
      PROFILE="${2:?missing value for --profile}"
      shift 2
      ;;
    --profile=*)
      PROFILE="${1#*=}"
      shift
      ;;
    --full)
      PROFILE="full"
      shift
      ;;
    --keep-artifacts)
      KEEP_ARTIFACTS=1
      shift
      ;;
    -h|--help)
      cat <<'USAGE'
Usage: scripts/spora_cellscript_acceptance.sh [--profile base|external-boot|cellscript|propagation|full|production] [--keep-artifacts]

Profiles:
  base           Run the in-process devnet base probe, including VM code-cell and CellScript ELF deploy/spend.
  external-boot  Generate a devnet wallet manifest and boot a real sporad process with explicit relaxed mass policy.
  cellscript     Run focused CellScript examples and package-manager acceptance tests.
  propagation    Run the two-node block/transaction propagation acceptance test.
  full           Run base, external-boot, propagation, and the focused CellScript tool/package suite.
  production     Run full acceptance and fail unless the Spora production gate reports production_ready=true.
USAGE
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_ID="$(date +%Y%m%d-%H%M%S)-$$"
STARTED_AT_UTC="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
RUN_DIR="$REPO_ROOT/target/devnet-acceptance/$RUN_ID"
BOOTSTRAP_JSON="$RUN_DIR/bootstrap.json"
SPORAD_LOG="$RUN_DIR/sporad.log"
PROBE_JSON="$RUN_DIR/probe-report.json"
BASE_REPORT_JSON="$RUN_DIR/base-report.json"
CELLSCRIPT_REPORT_JSON="$RUN_DIR/cellscript-report.json"
PROPAGATION_REPORT_JSON="$RUN_DIR/propagation-report.json"
REPORT_JSON="$RUN_DIR/acceptance-report.json"
PRODUCTION_EVIDENCE_JSON="$RUN_DIR/production-evidence.json"
NODE_PID=""
PREALLOC_ADDRESS=""
BASE_STATUS="skipped"
EXTERNAL_BOOT_STATUS="skipped"
CELLSCRIPT_STATUS="skipped"
PROPAGATION_STATUS="skipped"
GIT_REVISION="$(git -C "$REPO_ROOT" rev-parse --short HEAD 2>/dev/null || true)"
GIT_STATUS_COUNT="$(git -C "$REPO_ROOT" status --porcelain=v1 2>/dev/null | wc -l | awk '{print $1}')"
CURRENT_STEP="setup"
RESULT="passed"
FAILURE_EXIT_CODE=""
FAILURE_LINE=""

mkdir -p "$RUN_DIR"

stop_node() {
  if [[ -n "$NODE_PID" ]] && kill -0 "$NODE_PID" >/dev/null 2>&1; then
    kill "$NODE_PID" >/dev/null 2>&1 || true
    wait "$NODE_PID" >/dev/null 2>&1 || true
  fi
  NODE_PID=""
}

cleanup() {
  stop_node
}
trap cleanup EXIT

run_base() {
  local mass_policy_mode="${1:-relaxed}"
  local standard_mass_policy=0
  if [[ "$mass_policy_mode" == "standard" ]]; then
    standard_mass_policy=1
  elif [[ "$mass_policy_mode" != "relaxed" ]]; then
    echo "unsupported base mass policy mode: $mass_policy_mode" >&2
    exit 2
  fi
  (
    cd "$REPO_ROOT"
    DEVNET_ACCEPTANCE_BASE_REPORT_JSON="$BASE_REPORT_JSON" \
    DEVNET_ACCEPTANCE_STANDARD_MASS_POLICY="$standard_mass_policy" \
    cargo test --locked -p spora-testing-integration --lib \
      --features "integration-tests devnet-prealloc vm" \
      devnet_acceptance_tests::devnet_acceptance_base \
      -- --nocapture --test-threads=1
  )
  validate_base_report
  mark_json_status "$BASE_REPORT_JSON" "passed"
  BASE_STATUS="passed"
}

validate_base_report() {
  python3 - "$BASE_REPORT_JSON" <<'PY'
import json
import sys

path = sys.argv[1]
with open(path, "r", encoding="utf-8") as fh:
    report = json.load(fh)

expected_examples = [
    "amm_pool.cell",
    "launch.cell",
    "multisig.cell",
    "nft.cell",
    "timelock.cell",
    "token.cell",
    "vesting.cell",
]
examples = report.get("bundled_examples", [])
names = [item.get("name") for item in examples]
if names != expected_examples:
    raise SystemExit(f"unexpected bundled example deployment list: {names}")

mass_policy = report.get("relaxed_mass_policy", {})
policy_mode = mass_policy.get("mode", "relaxed")
if policy_mode not in {"relaxed", "standard"}:
    raise SystemExit(f"unexpected base mass policy mode: {policy_mode}")
required_true_fields = [
    "signed_transfer_confirmed",
    "multi_input_multi_output_confirmed",
    "parent_child_mempool_confirmed",
    "scheduler_tamper_rejected",
    "always_success_vm_spend_confirmed",
    "noop_cellscript_spend_confirmed",
    "cellscript_schema_output_spend_confirmed",
]
if policy_mode == "relaxed":
    required_true_fields.append("cellscript_parameterized_amount_spend_confirmed")
for field in required_true_fields:
    if report.get(field) is not True:
        raise SystemExit(f"base report field {field} was not true")

if policy_mode == "relaxed":
    if mass_policy.get("relay_non_standard") is not True:
        raise SystemExit("base report did not record relaxed non-standard relay")
    if mass_policy.get("block_max_mass") != 100000000:
        raise SystemExit(f"unexpected relaxed base block_max_mass: {mass_policy.get('block_max_mass')}")
else:
    if mass_policy.get("relay_non_standard") is not False:
        raise SystemExit("standard base report must not enable non-standard relay")
    if mass_policy.get("block_max_mass") != 2000000:
        raise SystemExit(f"unexpected standard base block_max_mass: {mass_policy.get('block_max_mass')}")
if mass_policy.get("applies_to_all_networks_when_explicitly_enabled") is not True:
    raise SystemExit("base report did not record all-network explicit opt-in scope")
if mass_policy.get("standard_policy_preserved_by_default") is not True:
    raise SystemExit("base report did not record default standard policy preservation")

for item in examples:
    name = item.get("name")
    if item.get("deployment_probe_status") not in {"accepted-indexed", "standard-policy-rejected"}:
        raise SystemExit(f"{name} recorded an unexpected deployment probe status: {item.get('deployment_probe_status')}")
    if item.get("malformed_spend_probe_status") not in {"script-rejected", "skipped-deployment-not-indexed"}:
        raise SystemExit(f"{name} recorded an unexpected malformed spend probe status: {item.get('malformed_spend_probe_status')}")
    if policy_mode == "relaxed":
        if not item.get("code_cell_indexed"):
            raise SystemExit(f"{name} code cell was not indexed according to base report")
        if not item.get("malformed_spend_rejected"):
            raise SystemExit(f"{name} malformed spend was not rejected according to base report")
        if item.get("malformed_spend_rejected_by_standard_policy"):
            raise SystemExit(f"{name} malformed spend was rejected by standard policy")
        reason = item.get("malformed_spend_reject_reason", "")
        lowered = reason.lower()
        forbidden = ["not standard", "storage mass", "compute mass", "transient", "cycles exceeded", "cycles limit"]
        if any(marker in lowered for marker in forbidden):
            raise SystemExit(f"{name} malformed spend did not fail fast in script/business validation: {reason}")
    else:
        if item.get("fits_standard_relay_transaction_mass"):
            if item.get("deployment_probe_status") != "accepted-indexed":
                raise SystemExit(f"{name} standard-compatible deployment was not indexed")
            if item.get("malformed_spend_probe_status") != "script-rejected":
                raise SystemExit(f"{name} standard-compatible malformed spend did not run script rejection probe")
            if item.get("malformed_spend_rejected_by_standard_policy"):
                raise SystemExit(f"{name} standard-compatible malformed spend was rejected by standard policy")
        else:
            if item.get("deployment_probe_status") != "standard-policy-rejected":
                raise SystemExit(f"{name} standard-incompatible deployment was not recorded as standard-policy-rejected")
            if item.get("malformed_spend_probe_status") != "skipped-deployment-not-indexed":
                raise SystemExit(f"{name} standard-incompatible malformed spend probe was not explicitly skipped")
    if not item.get("artifact_size_bytes", 0) > 0:
        raise SystemExit(f"{name} artifact size was not recorded")
    if not item.get("action_count", 0) > 0:
        raise SystemExit(f"{name} action coverage was not recorded")
    if len(item.get("action_names", [])) != item.get("action_count"):
        raise SystemExit(f"{name} action_names/action_count mismatch")
    for mass_field in [
        "estimated_storage_mass",
        "estimated_code_deployment_mass",
        "estimated_standard_deployment_storage_mass",
    ]:
        if not item.get(mass_field, 0) > 0:
            raise SystemExit(f"{name} {mass_field} was not recorded")
    if "fits_standard_relay_transaction_mass" not in item:
        raise SystemExit(f"{name} standard relay transaction compatibility was not recorded")

production_gate = report.get("production_gate", {})
if production_gate.get("status") not in {"blocked", "passed"}:
    raise SystemExit(f"unexpected Spora production gate status: {production_gate.get('status')}")
if production_gate.get("bundled_example_deployment_probe_count") != len(expected_examples):
    raise SystemExit("Spora production gate did not record one deployment probe per bundled example")
if production_gate.get("required_action_specific_builder_count", 0) <= 0:
    raise SystemExit("Spora production gate did not record required action-specific builder count")
coverage = production_gate.get("action_builder_coverage", [])
if len(coverage) != production_gate.get("required_action_specific_builder_count"):
    raise SystemExit("Spora production gate action coverage count mismatch")
if production_gate.get("scoped_action_artifact_count", -1) < 0:
    raise SystemExit("Spora production gate did not record scoped action artifact coverage")
if production_gate.get("scheduler_witness_shape_count", -1) < 0:
    raise SystemExit("Spora production gate did not record scheduler witness shape coverage")
if production_gate.get("scheduler_witness_shape_malformed_count", -1) < 0:
    raise SystemExit("Spora production gate did not record malformed scheduler witness shape coverage")
if production_gate.get("standard_block_max_mass", 0) <= 0:
    raise SystemExit("Spora production gate did not record standard block max mass")
if production_gate.get("standard_relay_max_tx_mass") != 500000:
    raise SystemExit("Spora production gate did not record standard relay max transaction mass")
if production_gate.get("relaxed_block_max_mass") != 100000000:
    raise SystemExit("Spora production gate did not record relaxed block max mass")
if policy_mode == "relaxed" and production_gate.get("relaxed_block_max_mass") != mass_policy.get("block_max_mass"):
    raise SystemExit("Spora production gate relaxed block max mass does not match the acceptance mass policy")
if production_gate.get("standard_relay_deploy_compatible_example_count", -1) < 0:
    raise SystemExit("Spora production gate did not record standard relay deployment compatibility")
if production_gate.get("standard_relay_deploy_compatible_action_count", -1) < 0:
    raise SystemExit("Spora production gate did not record scoped standard relay deployment compatibility")
if production_gate.get("bundled_example_count", -1) != len(expected_examples):
    raise SystemExit("Spora production gate did not record bundled example count")
if "full_file_monolith_standard_relay_ready" not in production_gate:
    raise SystemExit("Spora production gate did not record full-file monolith standard relay readiness")
if "scoped_action_standard_relay_ready" not in production_gate:
    raise SystemExit("Spora production gate did not record scoped action standard relay readiness")
if "standard_relay_incompatible_examples" not in production_gate:
    raise SystemExit("Spora production gate did not record standard relay incompatible examples")
if "advisories" not in production_gate:
    raise SystemExit("Spora production gate did not record advisory diagnostics")
for item in coverage:
    if "scoped_action_artifact_covered" not in item:
        raise SystemExit("Spora production gate action coverage is missing scoped action artifact coverage")
    if item.get("scoped_action_artifact_covered") and not item.get("scoped_action_artifact_bytes", 0) > 0:
        raise SystemExit("Spora production gate scoped action artifact did not record a positive artifact size")
    if item.get("scoped_action_artifact_covered") and not item.get("scoped_action_artifact_hash"):
        raise SystemExit("Spora production gate scoped action artifact did not record an artifact hash")
    if "scheduler_witness_shape_covered" not in item:
        raise SystemExit("Spora production gate action coverage is missing scheduler witness shape coverage")
    if "scheduler_witness_shape_malformed_covered" not in item:
        raise SystemExit("Spora production gate action coverage is missing malformed scheduler witness shape coverage")
    requirements = item.get("builder_requirements")
    if not isinstance(requirements, dict):
        raise SystemExit("Spora production gate action coverage is missing builder requirements")
    for requirement_field in [
        "entry_param_count",
        "scheduler_witness_bytes",
        "scheduler_access_count",
        "min_input_count",
        "min_cell_dep_count",
        "min_output_count",
        "consume_count",
        "read_ref_count",
        "create_count",
        "mutate_count",
        "transaction_runtime_input_requirement_count",
        "checked_runtime_obligation_count",
        "fail_closed_runtime_feature_count",
        "estimated_cycles",
        "estimated_standard_deployment_storage_mass",
    ]:
        if requirement_field not in requirements:
            raise SystemExit(f"Spora production gate builder requirements are missing {requirement_field}")
        if not isinstance(requirements[requirement_field], int) or requirements[requirement_field] < 0:
            raise SystemExit(f"Spora production gate builder requirement {requirement_field} must be a non-negative integer")
    if item.get("scheduler_witness_shape_covered") and requirements["scheduler_witness_bytes"] <= 0:
        raise SystemExit("Spora production gate builder requirements did not record scheduler witness bytes")
    if "fits_standard_relay_transaction_mass" not in requirements:
        raise SystemExit("Spora production gate builder requirements did not record standard relay transaction compatibility")
    if requirements.get("requires_action_specific_transaction_builder") is not True:
        raise SystemExit("Spora production gate builder requirements must require action-specific transaction builders")
if production_gate.get("production_ready") is True and production_gate.get("status") != "passed":
    raise SystemExit("Spora production gate cannot be production_ready without passed status")
if production_gate.get("production_ready") is False and not production_gate.get("blockers"):
    raise SystemExit("Spora production gate must explain blockers when not production-ready")
PY
}

validate_spora_production_ready() {
  python3 - "$BASE_REPORT_JSON" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as fh:
    report = json.load(fh)
gate = report.get("production_gate", {})
if gate.get("production_ready") is not True:
    blockers = gate.get("blockers", [])
    advisories = gate.get("advisories", [])
    message = "Spora production gate is not ready: " + "; ".join(blockers)
    if advisories:
        message += " | advisories: " + "; ".join(advisories)
    raise SystemExit(message)
PY
}

mark_json_status() {
  local path="$1"
  local status="$2"
  python3 - "$path" "$status" <<'PY'
import json
import sys

path, status = sys.argv[1], sys.argv[2]
with open(path, "r", encoding="utf-8") as fh:
    report = json.load(fh)
report["status"] = status
with open(path, "w", encoding="utf-8") as fh:
    json.dump(report, fh, indent=2)
    fh.write("\n")
PY
}

run_cellscript_suite() {
  (
    cd "$REPO_ROOT"
    cargo test --locked -p cellscript --test examples -- --nocapture --test-threads=1
    cargo test --locked -p cellscript --test cli cellc_compiles_package_with_local_path_dependency -- --nocapture --test-threads=1
    cargo test --locked -p cellscript --test cli cellc_rejects_registry_package_dependencies_fail_closed -- --nocapture --test-threads=1
    cargo test --locked -p cellscript --test cli cellc_build_and_check_subcommands_use_package_flow -- --nocapture --test-threads=1
    cargo test --locked -p cellscript --test cli cellc_init_subcommand_supports_json_summary -- --nocapture --test-threads=1
    cargo test --locked -p cellscript --test cli cellc_info_subcommand_supports_json_summary -- --nocapture --test-threads=1
    cargo test --locked -p cellscript --test cli cellc_entry_witness_subcommand -- --nocapture --test-threads=1
    cargo test --locked -p cellscript --test cli cellc_add_and_remove_subcommands_honor_dev_path_and_json -- --nocapture --test-threads=1
    cargo test --locked -p cellscript --test cli cellc_install_path_updates_lockfile_and_remove_prunes_it -- --nocapture --test-threads=1
    cargo test --locked -p spora-testing-integration --lib \
      --features "integration-tests devnet-prealloc vm" \
      common::cellscript_contracts::tests::all_spora_examples_compile_metadata_acceptance \
      -- --nocapture --test-threads=1
  )
  CELLSCRIPT_STATUS="passed"
  cat > "$CELLSCRIPT_REPORT_JSON" <<JSON
{
  "profile": "cellscript",
  "status": "passed",
  "tests": [
    "cellscript_examples_action_metadata_suite",
    "cellc_compiles_package_with_local_path_dependency",
    "cellc_rejects_registry_package_dependencies_fail_closed",
    "cellc_build_and_check_subcommands_use_package_flow",
    "cellc_init_subcommand_supports_json_summary",
    "cellc_info_subcommand_supports_json_summary",
    "cellc_entry_witness_subcommand",
    "cellc_add_and_remove_subcommands_honor_dev_path_and_json",
    "cellc_install_path_updates_lockfile_and_remove_prunes_it",
    "all_spora_examples_compile_metadata_acceptance"
  ]
}
JSON
}

run_propagation_suite() {
  (
    cd "$REPO_ROOT"
    cargo test --locked -p spora-testing-integration --lib \
      --features "integration-tests devnet-prealloc vm" \
      daemon_integration_tests::daemon_cells_propagation_test \
      -- --nocapture --test-threads=1
  )
  PROPAGATION_STATUS="passed"
  cat > "$PROPAGATION_REPORT_JSON" <<JSON
{
  "profile": "propagation",
  "status": "passed",
  "tests": [
    "daemon_cells_propagation_test"
  ],
  "coverage": [
    "two-node block relay",
    "two-node transaction acceptance",
    "cellindex consistency",
    "address-scoped CellsChanged notifications"
  ]
}
JSON
}

bootstrap_wallet() {
  (
    cd "$REPO_ROOT"
    cargo run --locked -p spora-testing-integration --bin spora-devnet-bootstrap -- \
      --network devnet \
      --wallet-dir "$RUN_DIR/wallet" \
      --wallet-name acceptance \
      --prealloc-cells 101 \
      --prealloc-amount-sau 10000000000 \
      --out "$BOOTSTRAP_JSON" > "$RUN_DIR/bootstrap.stdout.json"
  )
}

json_value() {
  python3 - "$1" "$2" <<'PY'
import json
import sys

path, dotted = sys.argv[1], sys.argv[2]
with open(path, "r", encoding="utf-8") as fh:
    value = json.load(fh)
for key in dotted.split("."):
    value = value[key]
print(value)
PY
}

run_external_boot() {
  bootstrap_wallet
  local address
  address="$(json_value "$BOOTSTRAP_JSON" "wallet.default_address")"
  PREALLOC_ADDRESS="$address"

  (
    cd "$REPO_ROOT"
    cargo run --locked --bin sporad --features devnet-prealloc -- \
      --devnet \
      --appdir "$RUN_DIR/sporad" \
      --num-prealloc-cells=101 \
      --prealloc-address="$address" \
      --prealloc-amount=10000000000 \
      --cellindex \
      --relaynonstd \
      --blockmaxmass=100000000 \
      --nodnsseed \
      --disable-upnp \
      --rpclisten=0.0.0.0:16610 \
      --rpclisten-borsh=0.0.0.0:17610 \
      --rpclisten-json=0.0.0.0:18610 \
      --enable-unsynced-mining \
      --skip-proof-of-work \
      --unsaferpc \
      --yes
  ) > "$SPORAD_LOG" 2>&1 &
  NODE_PID="$!"

  local started=0
  for _ in $(seq 1 600); do
    if ! kill -0 "$NODE_PID" >/dev/null 2>&1; then
      echo "sporad exited during external boot probe; see $SPORAD_LOG" >&2
      tail -n 80 "$SPORAD_LOG" >&2 || true
      exit 1
    fi
    if grep -q "GRPC Server starting" "$SPORAD_LOG" >/dev/null 2>&1 && [[ "$(grep -c "WRPC Server starting" "$SPORAD_LOG" || true)" -ge 2 ]]; then
      started=1
      break
    fi
    sleep 1
  done
  if [[ "$started" -ne 1 ]]; then
    echo "sporad did not reach RPC startup during external boot probe; see $SPORAD_LOG" >&2
    tail -n 80 "$SPORAD_LOG" >&2 || true
    exit 1
  fi

  (
    cd "$REPO_ROOT"
    cargo run --locked -p spora-testing-integration --features wrpc-probe --bin spora-devnet-probe -- \
      --grpc grpc://127.0.0.1:16610 \
      --wrpc-borsh ws://127.0.0.1:17610 \
      --wrpc-json ws://127.0.0.1:18610 \
      --address "$address" \
      --expected-prealloc-cells 101 \
      --expected-prealloc-amount-sau 10000000000 \
      --out "$PROBE_JSON" > "$RUN_DIR/probe.stdout.json"
  )

  EXTERNAL_BOOT_STATUS="passed"
  stop_node
}

write_acceptance_report() {
  PROFILE="$PROFILE" \
  RUN_ID="$RUN_ID" \
  RUN_DIR="$RUN_DIR" \
  STARTED_AT_UTC="$STARTED_AT_UTC" \
  COMPLETED_AT_UTC="$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  ACCEPTANCE_COMMAND="$ACCEPTANCE_COMMAND" \
  GIT_REVISION="$GIT_REVISION" \
  GIT_STATUS_COUNT="$GIT_STATUS_COUNT" \
  BOOTSTRAP_JSON="$BOOTSTRAP_JSON" \
  PROBE_JSON="$PROBE_JSON" \
  BASE_REPORT_JSON="$BASE_REPORT_JSON" \
  CELLSCRIPT_REPORT_JSON="$CELLSCRIPT_REPORT_JSON" \
  PROPAGATION_REPORT_JSON="$PROPAGATION_REPORT_JSON" \
  SPORAD_LOG="$SPORAD_LOG" \
  PREALLOC_ADDRESS="$PREALLOC_ADDRESS" \
  BASE_STATUS="$BASE_STATUS" \
  EXTERNAL_BOOT_STATUS="$EXTERNAL_BOOT_STATUS" \
  CELLSCRIPT_STATUS="$CELLSCRIPT_STATUS" \
  PROPAGATION_STATUS="$PROPAGATION_STATUS" \
  RESULT="$RESULT" \
  CURRENT_STEP="$CURRENT_STEP" \
  FAILURE_EXIT_CODE="$FAILURE_EXIT_CODE" \
  FAILURE_LINE="$FAILURE_LINE" \
  python3 - "$REPORT_JSON" <<'PY'
import json
import os
import sys


def existing(env_name):
    path = os.environ.get(env_name, "")
    return path if path and os.path.exists(path) else None


def optional_int(env_name):
    value = os.environ.get(env_name, "")
    return int(value) if value else None


report = {
    "profile": os.environ["PROFILE"],
    "status": os.environ["RESULT"],
    "run_id": os.environ["RUN_ID"],
    "run_dir": os.environ["RUN_DIR"],
    "generated_at_utc": os.environ["STARTED_AT_UTC"],
    "completed_at_utc": os.environ["COMPLETED_AT_UTC"],
    "command": os.environ["ACCEPTANCE_COMMAND"],
    "git_revision": os.environ.get("GIT_REVISION") or None,
    "git_dirty": os.environ.get("GIT_STATUS_COUNT", "0") != "0",
    "bootstrap": existing("BOOTSTRAP_JSON"),
    "probe": existing("PROBE_JSON"),
    "base_report": existing("BASE_REPORT_JSON"),
    "cellscript_report": existing("CELLSCRIPT_REPORT_JSON"),
    "propagation_report": existing("PROPAGATION_REPORT_JSON"),
    "sporad_log": existing("SPORAD_LOG"),
    "prealloc_address": os.environ.get("PREALLOC_ADDRESS") or None,
    "base": os.environ["BASE_STATUS"],
    "external_boot": os.environ["EXTERNAL_BOOT_STATUS"],
    "cellscript": os.environ["CELLSCRIPT_STATUS"],
    "propagation": os.environ["PROPAGATION_STATUS"],
    "profiles": {
        "base": os.environ["BASE_STATUS"],
        "external_boot": os.environ["EXTERNAL_BOOT_STATUS"],
        "propagation": os.environ["PROPAGATION_STATUS"],
        "cellscript": os.environ["CELLSCRIPT_STATUS"],
    },
    "result": os.environ["RESULT"],
    "failed_step": os.environ["CURRENT_STEP"] if os.environ["RESULT"] != "passed" else None,
    "failure_exit_code": optional_int("FAILURE_EXIT_CODE"),
    "failure_line": optional_int("FAILURE_LINE"),
}

with open(sys.argv[1], "w", encoding="utf-8") as fh:
    json.dump(report, fh, indent=2)
    fh.write("\n")
PY
}

write_production_evidence_report() {
  if [[ "$PROFILE" != "production" || "$RESULT" != "passed" ]]; then
    return 0
  fi
  PROFILE="$PROFILE" \
  RUN_ID="$RUN_ID" \
  RUN_DIR="$RUN_DIR" \
  STARTED_AT_UTC="$STARTED_AT_UTC" \
  COMPLETED_AT_UTC="$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  ACCEPTANCE_COMMAND="$ACCEPTANCE_COMMAND" \
  GIT_REVISION="$GIT_REVISION" \
  GIT_STATUS_COUNT="$GIT_STATUS_COUNT" \
  BASE_REPORT_JSON="$BASE_REPORT_JSON" \
  CELLSCRIPT_REPORT_JSON="$CELLSCRIPT_REPORT_JSON" \
  PROPAGATION_REPORT_JSON="$PROPAGATION_REPORT_JSON" \
  REPORT_JSON="$REPORT_JSON" \
  SPORAD_LOG="$SPORAD_LOG" \
  python3 - "$PRODUCTION_EVIDENCE_JSON" <<'PY'
import json
import os
import sys


def load(path):
    with open(path, "r", encoding="utf-8") as fh:
        return json.load(fh)


def existing(env_name):
    path = os.environ.get(env_name, "")
    return path if path and os.path.exists(path) else None


base_report_path = os.environ["BASE_REPORT_JSON"]
base_report = load(base_report_path)
gate = base_report.get("production_gate", {})

required_checks = {
    "production_gate_passed": gate.get("status") == "passed",
    "production_ready": gate.get("production_ready") is True,
    "standard_mass_policy_used": gate.get("standard_mass_policy_used") is True,
    "scoped_action_standard_relay_ready": gate.get("scoped_action_standard_relay_ready") is True,
    "full_file_monolith_standard_relay_ready": gate.get("full_file_monolith_standard_relay_ready") is True,
    "no_standard_relay_incompatible_examples": gate.get("standard_relay_incompatible_examples") == [],
}
if not all(required_checks.values()):
    failed = [name for name, passed in required_checks.items() if not passed]
    raise SystemExit(f"refusing to write production evidence; failed checks: {failed}")

evidence = {
    "schema": "spora-devnet-production-evidence-v1",
    "profile": os.environ["PROFILE"],
    "status": "passed",
    "run_id": os.environ["RUN_ID"],
    "run_dir": os.environ["RUN_DIR"],
    "generated_at_utc": os.environ["COMPLETED_AT_UTC"],
    "command": os.environ["ACCEPTANCE_COMMAND"],
    "git_revision": os.environ.get("GIT_REVISION") or None,
    "git_dirty": os.environ.get("GIT_STATUS_COUNT", "0") != "0",
    "artifacts": {
        "acceptance_report": existing("REPORT_JSON"),
        "base_report": existing("BASE_REPORT_JSON"),
        "cellscript_report": existing("CELLSCRIPT_REPORT_JSON"),
        "propagation_report": existing("PROPAGATION_REPORT_JSON"),
        "sporad_log": existing("SPORAD_LOG"),
    },
    "production_gate": {
        "status": gate.get("status"),
        "production_ready": gate.get("production_ready"),
        "standard_mass_policy_used": gate.get("standard_mass_policy_used"),
        "standard_block_max_mass": gate.get("standard_block_max_mass"),
        "standard_relay_max_tx_mass": gate.get("standard_relay_max_tx_mass"),
        "scoped_action_artifact_count": gate.get("scoped_action_artifact_count"),
        "valid_action_specific_builder_count": gate.get("valid_action_specific_builder_count"),
        "malformed_action_matrix_count": gate.get("malformed_action_matrix_count"),
        "standard_relay_deploy_compatible_example_count": gate.get("standard_relay_deploy_compatible_example_count"),
        "bundled_example_count": gate.get("bundled_example_count"),
        "scoped_action_standard_relay_ready": gate.get("scoped_action_standard_relay_ready"),
        "full_file_monolith_standard_relay_ready": gate.get("full_file_monolith_standard_relay_ready"),
        "standard_relay_incompatible_examples": gate.get("standard_relay_incompatible_examples", []),
        "blockers": gate.get("blockers", []),
        "advisories": gate.get("advisories", []),
    },
    "required_checks": required_checks,
}

with open(sys.argv[1], "w", encoding="utf-8") as fh:
    json.dump(evidence, fh, indent=2)
    fh.write("\n")
PY
}

on_error() {
  local exit_code="$?"
  local line_no="${1:-}"
  trap - ERR
  set +e
  RESULT="failed"
  FAILURE_EXIT_CODE="$exit_code"
  FAILURE_LINE="$line_no"
  write_acceptance_report
  echo "acceptance failed during step '$CURRENT_STEP' with exit code $exit_code; report: $REPORT_JSON" >&2
  exit "$exit_code"
}

trap 'on_error $LINENO' ERR

case "$PROFILE" in
  base)
    CURRENT_STEP="base"
    run_base
    ;;
  external-boot)
    CURRENT_STEP="external_boot"
    run_external_boot
    ;;
  cellscript)
    CURRENT_STEP="cellscript"
    run_cellscript_suite
    ;;
  propagation)
    CURRENT_STEP="propagation"
    run_propagation_suite
    ;;
  full)
    CURRENT_STEP="base"
    run_base
    CURRENT_STEP="external_boot"
    run_external_boot
    CURRENT_STEP="propagation"
    run_propagation_suite
    CURRENT_STEP="cellscript"
    run_cellscript_suite
    ;;
  production)
    CURRENT_STEP="base"
    run_base standard
    CURRENT_STEP="spora_production_gate"
    validate_spora_production_ready
    CURRENT_STEP="external_boot"
    run_external_boot
    CURRENT_STEP="propagation"
    run_propagation_suite
    CURRENT_STEP="cellscript"
    run_cellscript_suite
    ;;
  *)
    echo "unsupported profile: $PROFILE" >&2
    exit 2
    ;;
esac

write_acceptance_report
write_production_evidence_report
if [[ -f "$PRODUCTION_EVIDENCE_JSON" ]]; then
  CURRENT_STEP="spora_production_evidence_validation"
  python3 "$REPO_ROOT/scripts/validate_spora_production_evidence.py" "$PRODUCTION_EVIDENCE_JSON"
fi

if [[ "$KEEP_ARTIFACTS" -eq 1 ]]; then
  echo "acceptance artifacts: $RUN_DIR"
else
  echo "acceptance artifacts: $RUN_DIR"
fi
if [[ -f "$PRODUCTION_EVIDENCE_JSON" ]]; then
  echo "production evidence: $PRODUCTION_EVIDENCE_JSON"
fi
