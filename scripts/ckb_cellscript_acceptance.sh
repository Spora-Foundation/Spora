#!/usr/bin/env bash
set -Eeuo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CKB_REPO="${CKB_REPO:-$(cd "$REPO_ROOT/.." && pwd)/ckb}"
CKB_BIN="${CKB_BIN:-}"
RUN_ONCHAIN=1
KEEP_NODE_LOGS=1
RUN_ID="$(date +%Y%m%d-%H%M%S)-$$"
RUN_DIR="$REPO_ROOT/target/ckb-cellscript-acceptance/$RUN_ID"
CKB_DIR="$RUN_DIR/ckb-node"
CKB_LOG="$RUN_DIR/ckb.log"
REPORT_JSON="$RUN_DIR/ckb-cellscript-acceptance-report.json"
CKB_PID=""

usage() {
  cat <<'USAGE'
Usage: scripts/ckb_cellscript_acceptance.sh [--ckb-repo <path>] [--ckb-bin <path>] [--compile-only]

Runs the bounded CellScript CKB compatibility acceptance against a local CKB
integration devnet from the parent CKB repository.

Options:
  --ckb-repo <path>   Parent CKB checkout. Defaults to ../ckb.
  --ckb-bin <path>    Existing CKB executable. Defaults to target/debug/ckb,
                      building `cargo build --bin ckb` in --ckb-repo if needed.
  --compile-only      Compile and verify the CKB-profile CellScript artifacts,
                      but skip local CKB node deployment/spend checks. This
                      mode does not require a CKB checkout or executable.
  -h, --help          Show this help.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --ckb-repo)
      CKB_REPO="${2:?missing value for --ckb-repo}"
      shift 2
      ;;
    --ckb-repo=*)
      CKB_REPO="${1#*=}"
      shift
      ;;
    --ckb-bin)
      CKB_BIN="${2:?missing value for --ckb-bin}"
      shift 2
      ;;
    --ckb-bin=*)
      CKB_BIN="${1#*=}"
      shift
      ;;
    --compile-only)
      RUN_ONCHAIN=0
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing required command: $1" >&2
    exit 127
  fi
}

pick_port() {
  python3 - <<'PY'
import socket

with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
    sock.bind(("127.0.0.1", 0))
    print(sock.getsockname()[1])
PY
}

resolve_ckb_bin() {
  if [[ -n "$CKB_BIN" ]]; then
    if [[ ! -x "$CKB_BIN" ]]; then
      echo "CKB_BIN is not executable: $CKB_BIN" >&2
      exit 1
    fi
    printf '%s\n' "$CKB_BIN"
    return
  fi

  local candidate
  for candidate in "$CKB_REPO/target/debug/ckb" "$CKB_REPO/target/release/ckb"; do
    if [[ -x "$candidate" ]]; then
      printf '%s\n' "$candidate"
      return
    fi
  done

  echo "No existing CKB executable found; building parent CKB checkout with cargo build --bin ckb" >&2
  (cd "$CKB_REPO" && cargo build --bin ckb)
  candidate="$CKB_REPO/target/debug/ckb"
  if [[ ! -x "$candidate" ]]; then
    echo "CKB build finished but executable was not found at $candidate" >&2
    exit 1
  fi
  printf '%s\n' "$candidate"
}

stop_ckb() {
  if [[ -n "$CKB_PID" ]] && kill -0 "$CKB_PID" >/dev/null 2>&1; then
    kill "$CKB_PID" >/dev/null 2>&1 || true
    wait "$CKB_PID" >/dev/null 2>&1 || true
  fi
  CKB_PID=""
}

cleanup() {
  stop_ckb
  if [[ "$KEEP_NODE_LOGS" != "1" && -f "$CKB_LOG" ]]; then
    rm -f "$CKB_LOG"
  fi
}
trap cleanup EXIT

require_cmd cargo
require_cmd python3
if [[ "$RUN_ONCHAIN" == "1" ]]; then
  require_cmd curl
fi

mkdir -p "$RUN_DIR"

RPC_URL=""
if [[ "$RUN_ONCHAIN" == "1" ]]; then
  if [[ ! -d "$CKB_REPO" ]]; then
    echo "CKB repo does not exist: $CKB_REPO" >&2
    exit 1
  fi
  if [[ ! -f "$CKB_REPO/test/template/ckb.toml" ]]; then
    echo "CKB repo does not contain test/template/ckb.toml: $CKB_REPO" >&2
    exit 1
  fi

  CKB_BIN="$(resolve_ckb_bin)"
  CKB_REPO="$(cd "$CKB_REPO" && pwd)"
  CKB_BIN="$(cd "$(dirname "$CKB_BIN")" && pwd)/$(basename "$CKB_BIN")"
  RPC_PORT="$(pick_port)"
  P2P_PORT="$(pick_port)"
  RPC_URL="http://127.0.0.1:$RPC_PORT"

  mkdir -p "$CKB_DIR"
  cp -R "$CKB_REPO/test/template/." "$CKB_DIR/"

  python3 - "$CKB_DIR/ckb.toml" "$RPC_PORT" "$P2P_PORT" <<'PY'
import pathlib
import re
import sys

path = pathlib.Path(sys.argv[1])
rpc_port = sys.argv[2]
p2p_port = sys.argv[3]
text = path.read_text(encoding="utf-8")
text = re.sub(
    r'listen_address = "127\.0\.0\.1:\d+"',
    f'listen_address = "127.0.0.1:{rpc_port}"',
    text,
    count=1,
)
text = re.sub(
    r'listen_addresses = \["/ip4/0\.0\.0\.0/tcp/\d+"\]',
    f'listen_addresses = ["/ip4/127.0.0.1/tcp/{p2p_port}"]',
    text,
    count=1,
)
path.write_text(text, encoding="utf-8")
PY
else
  if [[ -d "$CKB_REPO" ]]; then
    CKB_REPO="$(cd "$CKB_REPO" && pwd)"
  fi
  if [[ -n "$CKB_BIN" && -e "$CKB_BIN" ]]; then
    CKB_BIN="$(cd "$(dirname "$CKB_BIN")" && pwd)/$(basename "$CKB_BIN")"
  fi
fi

CARGO_TARGET_DIR_RESOLVED="${CARGO_TARGET_DIR:-$REPO_ROOT/target}"
case "$CARGO_TARGET_DIR_RESOLVED" in
  /*) ;;
  *) CARGO_TARGET_DIR_RESOLVED="$REPO_ROOT/$CARGO_TARGET_DIR_RESOLVED" ;;
esac
CELLC_BIN="$CARGO_TARGET_DIR_RESOLVED/debug/cellc"
cargo build --manifest-path "$REPO_ROOT/Cargo.toml" -p cellscript --bin cellc
if [[ ! -x "$CELLC_BIN" ]]; then
  echo "cellc build finished but executable was not found at $CELLC_BIN" >&2
  exit 1
fi

python3 - "$CELLC_BIN" "$REPO_ROOT" "$RUN_DIR" "$REPORT_JSON" <<'PY'
import json
import os
import pathlib
import shutil
import subprocess
import sys

cellc = pathlib.Path(sys.argv[1])
repo_root = pathlib.Path(sys.argv[2])
run_dir = pathlib.Path(sys.argv[3])
report_path = pathlib.Path(sys.argv[4])

EXAMPLES = [
    "amm_pool.cell",
    "launch.cell",
    "multisig.cell",
    "nft.cell",
    "timelock.cell",
    "token.cell",
    "vesting.cell",
]
BYPASS_ENV = "CELLSCRIPT_CKB_ACCEPTANCE_SMOKE_ALLOW_UNPORTABLE_EXAMPLES"
TRUNCATE = 12000

examples_dir = repo_root / "cellscript" / "examples"
actual_examples = sorted(path.name for path in examples_dir.glob("*.cell") if path.is_file())
if actual_examples != sorted(EXAMPLES):
    raise SystemExit(f"bundled examples changed: expected {sorted(EXAMPLES)}, found {actual_examples}")

source_root = run_dir / "smoke-sources"
example_source_root = source_root / "examples"
baseline_source_root = source_root / "baseline"
artifact_root = run_dir / "artifacts"
strict_root = run_dir / "strict-original-ckb"
for path in (example_source_root, baseline_source_root, artifact_root, strict_root):
    path.mkdir(parents=True, exist_ok=True)

baseline_source = baseline_source_root / "ckb_noop.cell"
baseline_source.write_text(
    """module acceptance::ckb_noop

action main() -> u64 {
    0
}
""",
    encoding="utf-8",
)

for name in EXAMPLES:
    original = examples_dir / name
    smoke_source = example_source_root / name
    smoke_source.write_text(
        original.read_text(encoding="utf-8")
        + """

action main() -> u64 {
    0
}
""",
        encoding="utf-8",
    )

def clipped(text):
    if len(text) <= TRUNCATE:
        return text
    return text[:TRUNCATE] + f"\n... truncated {len(text) - TRUNCATE} bytes ..."

def run(args, *, env=None, timeout=180):
    completed = subprocess.run(args, env=env, text=True, capture_output=True, timeout=timeout)
    return {
        "command": [str(arg) for arg in args],
        "returncode": completed.returncode,
        "stdout": clipped(completed.stdout),
        "stderr": clipped(completed.stderr),
    }

def load_json(path):
    return json.loads(path.read_text(encoding="utf-8"))

def verify_artifact(artifact):
    completed = subprocess.run(
        [cellc, "verify-artifact", artifact, "--expect-target-profile", "ckb", "--json"],
        text=True,
        capture_output=True,
        timeout=180,
    )
    if completed.returncode != 0:
        raise RuntimeError(f"verify-artifact failed for {artifact}: {clipped(completed.stderr)}")
    try:
        return json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        raise RuntimeError(f"verify-artifact did not return JSON for {artifact}: {clipped(completed.stdout)}") from error

def compile_artifact(name, kind, source, artifact, *, bypass_policy):
    env = os.environ.copy()
    if bypass_policy:
        env[BYPASS_ENV] = "1"
    result = run([cellc, source, "--target-profile", "ckb", "--target", "riscv64-elf", "-o", artifact], env=env)
    if result["returncode"] != 0:
        raise RuntimeError(f"CKB smoke compile failed for {name}: {result['stderr']}")
    if not artifact.exists():
        raise RuntimeError(f"CKB smoke compile did not produce artifact for {name}: {artifact}")

    metadata_path = pathlib.Path(str(artifact) + ".meta.json")
    if not metadata_path.exists():
        raise RuntimeError(f"CKB smoke compile did not produce metadata sidecar for {name}: {metadata_path}")

    artifact_bytes = artifact.read_bytes()
    artifact_has_sporabi_trailer = b"SPORABI" in artifact_bytes[-64:]
    if not artifact_bytes.startswith(b"\x7fELF"):
        raise RuntimeError(f"{name} artifact is not an ELF")
    if artifact_has_sporabi_trailer:
        raise RuntimeError(f"{name} CKB artifact still contains a SPORABI trailer")

    metadata = load_json(metadata_path)
    verify = verify_artifact(artifact)
    if metadata.get("target_profile", {}).get("name") != "ckb" or verify.get("target_profile") != "ckb":
        raise RuntimeError(f"{name} metadata/verify did not pin target_profile=ckb")

    return {
        "name": name,
        "kind": kind,
        "source": str(source),
        "artifact": str(artifact),
        "metadata": str(metadata_path),
        "artifact_size_bytes": len(artifact_bytes),
        "artifact_starts_with_elf_magic": True,
        "artifact_has_sporabi_trailer": False,
        "target_profile": "ckb",
        "artifact_packaging": metadata.get("target_profile", {}).get("artifact_packaging"),
        "acceptance_smoke_policy_bypass": bypass_policy,
        "compile": result,
        "verify": verify,
    }

def strict_original_compile(name):
    source = examples_dir / name
    artifact = strict_root / f"{name}.strict.elf"
    result = run([cellc, source, "--target-profile", "ckb", "--target", "riscv64-elf", "-o", artifact])
    policy_fail_closed = result["returncode"] != 0 and "target profile policy failed for 'ckb'" in result["stderr"]
    unexpected_failure = result["returncode"] != 0 and not policy_fail_closed
    return {
        "source": str(source),
        "artifact": str(artifact),
        "status": "passed" if result["returncode"] == 0 else "failed",
        "policy_fail_closed": policy_fail_closed,
        "unexpected_failure": unexpected_failure,
        "returncode": result["returncode"],
        "stdout": result["stdout"],
        "stderr": result["stderr"],
    }

artifacts = []
baseline = compile_artifact(
    "ckb_noop.cell",
    "pure-baseline",
    baseline_source,
    artifact_root / "ckb_noop.elf",
    bypass_policy=False,
)
artifacts.append(baseline)

bundled_examples = []
for name in EXAMPLES:
    strict = strict_original_compile(name)
    if strict["unexpected_failure"]:
        raise RuntimeError(
            f"strict original CKB compile for {name} failed for a non-policy reason: {strict['stderr']}"
        )
    record = compile_artifact(
        name,
        "bundled-example-smoke",
        example_source_root / name,
        artifact_root / f"{name}.elf",
        bypass_policy=True,
    )
    record["original_source"] = str(examples_dir / name)
    record["strict_original_ckb_compile"] = strict
    bundled_examples.append(record)
    artifacts.append(record)

strict_original_policy_fail_closed = [
    record["name"]
    for record in bundled_examples
    if record["strict_original_ckb_compile"]["policy_fail_closed"]
]
strict_original_unexpected_failures = [
    record["name"]
    for record in bundled_examples
    if record["strict_original_ckb_compile"]["unexpected_failure"]
]

report = {
    "status": "artifact-verified",
    "ckb_acceptance_scope": (
        "Full-mode CKB devnet acceptance deploys and spends every emitted code cell. "
        "Bundled examples use an appended no-argument main smoke entry so CKB-VM execution, "
        "artifact packaging, code-cell dependency resolution, and lock-script invocation are tested; "
        "strict original business-action portability remains recorded separately."
    ),
    "cellc": str(cellc),
    "bundled_examples_exact_order": EXAMPLES,
    "bundled_examples_count": len(EXAMPLES),
    "all_bundled_examples_smoke_compiled": all(record["kind"] == "bundled-example-smoke" for record in bundled_examples),
    "strict_original_ckb_compile_policy_fail_closed": strict_original_policy_fail_closed,
    "strict_original_ckb_compile_unexpected_failures": strict_original_unexpected_failures,
    "acceptance_smoke_policy_bypass_env": BYPASS_ENV,
    "pure_baseline": baseline,
    "bundled_examples": bundled_examples,
    "artifacts": artifacts,
}
report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY

if [[ "$RUN_ONCHAIN" != "1" ]]; then
  python3 - "$REPORT_JSON" "$CKB_REPO" "$CKB_BIN" "$RPC_URL" <<'PY'
import json
import pathlib
import sys

report_path = pathlib.Path(sys.argv[1])
report = json.loads(report_path.read_text(encoding="utf-8"))
report.update({
    "status": "passed",
    "ckb_repo": sys.argv[2],
    "ckb_bin": sys.argv[3],
    "rpc_url": sys.argv[4],
    "onchain": {"status": "skipped", "reason": "compile-only"},
})
report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY
  echo "CKB CellScript compile-only acceptance passed: $REPORT_JSON"
  exit 0
fi

"$CKB_BIN" -C "$CKB_DIR" run --ba-advanced > "$CKB_LOG" 2>&1 &
CKB_PID="$!"

ready=0
for _ in $(seq 1 120); do
  if curl -sS \
    -H 'Content-Type: application/json' \
    -d '{"id":1,"jsonrpc":"2.0","method":"get_tip_header","params":[]}' \
    "$RPC_URL" > "$RUN_DIR/rpc-ready.json" 2>/dev/null; then
    if python3 - "$RUN_DIR/rpc-ready.json" <<'PY'
import json
import pathlib
import sys

payload = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
raise SystemExit(0 if payload.get("result") and not payload.get("error") else 1)
PY
    then
      ready=1
      break
    fi
  fi
  if ! kill -0 "$CKB_PID" >/dev/null 2>&1; then
    echo "CKB process exited before RPC became ready. Log: $CKB_LOG" >&2
    tail -100 "$CKB_LOG" >&2 || true
    exit 1
  fi
  sleep 1
done

if [[ "$ready" != "1" ]]; then
  echo "CKB RPC did not become ready at $RPC_URL. Log: $CKB_LOG" >&2
  tail -100 "$CKB_LOG" >&2 || true
  exit 1
fi

python3 - "$RPC_URL" "$REPORT_JSON" "$CKB_REPO" "$CKB_BIN" "$CKB_LOG" <<'PY'
import hashlib
import json
import pathlib
import sys
import time
import urllib.error
import urllib.request

rpc_url, report_path, ckb_repo, ckb_bin, ckb_log = sys.argv[1:]
report_path = pathlib.Path(report_path)

ALWAYS_SUCCESS_CODE_HASH = "0x28e83a1277d48add8e72fadaa9248559e1b632bab2bd60b27955ebc4c03800a5"
ALWAYS_SUCCESS_INDEX = "0x5"

report = json.loads(report_path.read_text(encoding="utf-8"))
artifacts = report.get("artifacts", [])
if not artifacts:
    raise RuntimeError("acceptance report does not contain artifacts")

report.update({
    "status": "running-onchain",
    "ckb_repo": ckb_repo,
    "ckb_bin": ckb_bin,
    "ckb_log": ckb_log,
    "rpc_url": rpc_url,
    "onchain": {
        "status": "running",
        "chain_template": "ckb/test/template integration devnet",
        "always_success_system_cell_index": ALWAYS_SUCCESS_INDEX,
        "artifact_runs": [],
    },
})

def write_report():
    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")

def rpc(method, params=None):
    body = json.dumps({"id": 42, "jsonrpc": "2.0", "method": method, "params": params or []}).encode("utf-8")
    request = urllib.request.Request(rpc_url, data=body, headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(request, timeout=20) as response:
            payload = json.loads(response.read().decode("utf-8"))
    except urllib.error.URLError as error:
        raise RuntimeError(f"RPC {method} failed to connect: {error}") from error
    if payload.get("error"):
        raise RuntimeError(f"RPC {method} returned error: {payload['error']}")
    return payload.get("result")

def hex_u64(value):
    if isinstance(value, str):
        value = int(value, 16)
    return hex(value)

def out_point(tx_hash, index):
    return {"tx_hash": tx_hash, "index": hex_u64(index)}

def always_success_lock():
    return {"code_hash": ALWAYS_SUCCESS_CODE_HASH, "hash_type": "data", "args": "0x"}

def data_hash(data):
    return "0x" + hashlib.blake2b(data, digest_size=32, person=b"ckb-default-hash").hexdigest()

def get_block(block_hash):
    block = rpc("get_block", [block_hash])
    if block is None:
        raise RuntimeError(f"block not found: {block_hash}")
    return block

def get_block_by_number(number):
    block = rpc("get_block_by_number", [hex_u64(number)])
    if block is None:
        raise RuntimeError(f"block number not found: {number}")
    return block

def find_spendable_cellbase(max_blocks=64):
    generated = []
    for _ in range(max_blocks):
        block_hash = rpc("generate_block")
        generated.append(block_hash)
        block = get_block(block_hash)
        cellbase = block["transactions"][0]
        outputs = cellbase.get("outputs", [])
        if outputs:
            for index, output in enumerate(outputs):
                capacity = int(output["capacity"], 16)
                if capacity > 0:
                    return {
                        "block_hash": block_hash,
                        "tx_hash": cellbase["hash"],
                        "index": index,
                        "capacity": capacity,
                        "generated_blocks": generated,
                    }
    raise RuntimeError(f"no spendable cellbase output found after {max_blocks} generated blocks")

def collect_spendable_cellbases(min_capacity, max_cells=256):
    cells = []
    total_capacity = 0
    generated_blocks = []
    while total_capacity < min_capacity and len(cells) < max_cells:
        cell = find_spendable_cellbase()
        cells.append(cell)
        total_capacity += cell["capacity"]
        generated_blocks.extend(cell["generated_blocks"])
    if total_capacity < min_capacity:
        raise RuntimeError(
            f"collected {total_capacity:#x} capacity from {len(cells)} cellbase cells, "
            f"need at least {min_capacity:#x}"
        )
    return {
        "cells": cells,
        "total_capacity": total_capacity,
        "generated_blocks": generated_blocks,
    }

def transaction(input_cells, outputs, outputs_data, cell_deps):
    if isinstance(input_cells, dict) and "cells" in input_cells:
        input_cells = input_cells["cells"]
    elif isinstance(input_cells, dict):
        input_cells = [input_cells]
    return {
        "version": "0x0",
        "cell_deps": cell_deps,
        "header_deps": [],
        "inputs": [
            {
                "previous_output": out_point(input_cell["tx_hash"], input_cell["index"]),
                "since": "0x0",
            }
            for input_cell in input_cells
        ],
        "outputs": outputs,
        "outputs_data": outputs_data,
        "witnesses": [],
    }

def submit_and_commit(tx, label, max_blocks=64):
    tx_hash = rpc("send_test_transaction", [tx, "passthrough"])
    for generated in range(max_blocks + 1):
        status = rpc("get_transaction", [tx_hash])
        tx_status = (status or {}).get("tx_status", {})
        if tx_status.get("status") == "committed":
            return {"tx_hash": tx_hash, "generated_blocks_after_submit": generated, "status": tx_status}
        rpc("generate_block")
        time.sleep(0.05)
    raise RuntimeError(f"{label} was not committed after {max_blocks} generated blocks: {tx_hash}")

def expect_dry_run_rejected(tx, label, expected_fragments):
    try:
        estimate = rpc("dry_run_transaction", [tx])
    except RuntimeError as error:
        message = str(error)
        if not any(fragment in message for fragment in expected_fragments):
            raise RuntimeError(f"{label} was rejected for an unexpected reason: {message}") from error
        forbidden_fragments = (
            "InsufficientCellCapacity",
            "ExceededMaximumAncestorsCount",
            "ExceededMaximumCycles",
            "MaxBlockCycles",
            "MaxBlockBytes",
            "Duplicated",
            "PoolIsFull",
        )
        if any(fragment in message for fragment in forbidden_fragments):
            raise RuntimeError(f"{label} was rejected by a policy/capacity reason: {message}") from error
        return {
            "status": "rejected",
            "check": "dry_run_transaction",
            "reason": message,
            "expected_reason_matched": True,
            "policy_or_capacity_reason": False,
        }
    raise RuntimeError(f"{label} was unexpectedly accepted by dry-run: {estimate}")

def assert_live(tx_hash, index, label):
    result = rpc("get_live_cell", [out_point(tx_hash, index), True])
    if not result or result.get("status") != "live":
        raise RuntimeError(f"{label} is not live: {result}")
    return result

def run_artifact(artifact_record, always_success_dep):
    name = artifact_record["name"]
    artifact_path = pathlib.Path(artifact_record["artifact"])
    artifact = artifact_path.read_bytes()
    artifact_ckb_data_hash = data_hash(artifact)

    result = {
        "name": name,
        "kind": artifact_record["kind"],
        "artifact": str(artifact_path),
        "artifact_size_bytes": len(artifact),
        "artifact_ckb_data_hash_blake2b": artifact_ckb_data_hash,
        "artifact_has_sporabi_trailer": b"SPORABI" in artifact[-64:],
        "acceptance_smoke_policy_bypass": artifact_record.get("acceptance_smoke_policy_bypass", False),
    }
    if result["artifact_has_sporabi_trailer"]:
        raise RuntimeError(f"{name} CKB artifact still contains a SPORABI trailer")

    deploy_min_capacity = (len(artifact) + 1_000) * 100_000_000
    deploy_input = collect_spendable_cellbases(deploy_min_capacity)
    deploy_tx = transaction(
        deploy_input,
        [
            {
                "capacity": hex_u64(deploy_input["total_capacity"]),
                "lock": always_success_lock(),
                "type": None,
            }
        ],
        ["0x" + artifact.hex()],
        [always_success_dep],
    )
    deploy_result = submit_and_commit(deploy_tx, f"{name} code-cell deploy")
    deploy_live = assert_live(deploy_result["tx_hash"], 0, f"{name} code cell")
    code_dep = {"out_point": out_point(deploy_result["tx_hash"], 0), "dep_type": "code"}
    result.update({
        "deploy_input": deploy_input,
        "code_cell_deploy": deploy_result,
        "code_cell_live": deploy_live.get("status") == "live",
        "code_cell_dep": code_dep,
    })

    create_input = collect_spendable_cellbases(100 * 100_000_000, max_cells=1)
    cellscript_lock = {"code_hash": artifact_ckb_data_hash, "hash_type": "data", "args": "0x"}
    create_tx = transaction(
        create_input,
        [
            {
                "capacity": hex_u64(create_input["total_capacity"]),
                "lock": cellscript_lock,
                "type": None,
            }
        ],
        ["0x"],
        [always_success_dep],
    )
    create_result = submit_and_commit(create_tx, f"{name} locked-cell create")
    create_live = assert_live(create_result["tx_hash"], 0, f"{name} locked cell")
    result.update({
        "create_input": create_input,
        "locked_cell_create": create_result,
        "locked_cell_live": create_live.get("status") == "live",
    })

    spend_input = {"tx_hash": create_result["tx_hash"], "index": 0, "capacity": create_input["total_capacity"]}
    missing_dep_spend_tx = transaction(
        spend_input,
        [
            {
                "capacity": hex_u64(spend_input["capacity"]),
                "lock": always_success_lock(),
                "type": None,
            }
        ],
        ["0x"],
        [],
    )
    missing_dep_rejection = expect_dry_run_rejected(
        missing_dep_spend_tx,
        f"{name} locked-cell spend without code cell dep",
        ("Resolve", "resolve", "Script", "script", "CellDep", "cell_dep", "code hash"),
    )
    still_live_after_reject = assert_live(create_result["tx_hash"], 0, f"{name} locked cell after malformed spend")
    result.update({
        "malformed_spend_without_code_dep": missing_dep_rejection,
        "locked_cell_live_after_malformed_spend": still_live_after_reject.get("status") == "live",
    })

    spend_tx = transaction(
        spend_input,
        [
            {
                "capacity": hex_u64(spend_input["capacity"]),
                "lock": always_success_lock(),
                "type": None,
            }
        ],
        ["0x"],
        [code_dep],
    )
    valid_spend_dry_run = rpc("dry_run_transaction", [spend_tx])
    spend_result = submit_and_commit(spend_tx, f"{name} locked-cell spend")
    spend_live = assert_live(spend_result["tx_hash"], 0, f"{name} spend recipient")
    result.update({
        "valid_spend_dry_run": valid_spend_dry_run,
        "locked_cell_spend": spend_result,
        "spend_recipient_live": spend_live.get("status") == "live",
    })
    return result

try:
    tip_before = rpc("get_tip_header")
    genesis = get_block_by_number(0)
    genesis_cellbase_hash = genesis["transactions"][0]["hash"]
    always_success_dep = {
        "out_point": out_point(genesis_cellbase_hash, int(ALWAYS_SUCCESS_INDEX, 16)),
        "dep_type": "code",
    }
    report["onchain"].update({
        "tip_before": tip_before,
        "genesis_cellbase_hash": genesis_cellbase_hash,
    })
    write_report()

    for artifact_record in artifacts:
        artifact_result = run_artifact(artifact_record, always_success_dep)
        report["onchain"]["artifact_runs"].append(artifact_result)
        report["onchain"]["completed_artifacts"] = len(report["onchain"]["artifact_runs"])
        write_report()

    tip_after = rpc("get_tip_header")
    report["status"] = "passed"
    report["onchain"]["status"] = "passed"
    report["onchain"]["tip_after"] = tip_after
    report["onchain"]["all_artifacts_deployed_and_spent"] = True
    report["onchain"]["bundled_examples_deployed_and_spent"] = [
        run["name"] for run in report["onchain"]["artifact_runs"] if run["kind"] == "bundled-example-smoke"
    ]
    write_report()
except Exception as error:
    report["status"] = "failed"
    report["onchain"]["status"] = "failed"
    report["onchain"]["error"] = str(error)
    write_report()
    raise
PY

echo "CKB CellScript acceptance passed: $REPORT_JSON"
