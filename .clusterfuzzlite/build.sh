#!/bin/bash -eu
# ClusterFuzzLite build — $SRC is the repo root, $OUT receives fuzz binaries.

cd "$SRC"

cargo fuzz build -O --target x86_64-unknown-linux-gnu

release_dir="$(python3 - <<'PY'
import json, os, subprocess, sys

meta = json.loads(
    subprocess.check_output(
        [
            "cargo",
            "metadata",
            "--manifest-path",
            "fuzz/Cargo.toml",
            "--format-version",
            "1",
            "--no-deps",
        ],
        text=True,
    )
)
print(os.path.join(meta["target_directory"], "x86_64-unknown-linux-gnu", "release"))
PY
)"

for f in fuzz/fuzz_targets/*.rs; do
  name="$(basename "${f%.*}")"
  cp "$release_dir/$name" "$OUT/"

  if [ -d "fuzz/seeds/$name" ]; then
    cp -r "fuzz/seeds/$name" "$OUT/${name}_seed_corpus"
  elif [ -d "fuzz/corpus/$name" ]; then
    cp -r "fuzz/corpus/$name" "$OUT/${name}_seed_corpus"
  fi

  if [ -f "fuzz/corvid.dict" ]; then
    cp fuzz/corvid.dict "$OUT/${name}.dict"
  fi
done
