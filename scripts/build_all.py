import subprocess
import os
import sys

# 🛡️ TylluanNexus o3 — Multi-Platform Build Orchestrator

TARGETS = [
    "x86_64-pc-windows-msvc",
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
    "x86_64-apple-darwin"
]

def build_target(target):
    print(f"🏗️  Building for {target}...")
    
    # Check if we need 'cross' for Linux/ARM on Windows
    cmd = ["cargo", "build", "--release", "--target", target, "-p", "tylluan-nexus", "-p", "tylluan-cli"]
    
    if "linux" in target or "aarch64" in target:
        if os.name == 'nt':
            print(f"ℹ️  Detected Windows host, using 'cross' for {target}")
            cmd[0] = "cross"

    try:
        subprocess.run(cmd, check=True)
        print(f"✅ Successfully built {target}")
    except subprocess.CalledProcessError:
        print(f"❌ Failed to build {target}")

if __name__ == "__main__":
    print("--------------------------------------------------")
    print("🛡️ TYLLUANNEXUS o3 — Cross-Platform Build")
    print("--------------------------------------------------")
    
    selected_targets = sys.argv[1:] if len(sys.argv) > 1 else TARGETS
    
    for target in selected_targets:
        build_target(target)

    print("--------------------------------------------------")
    print("🏁 Build cycle completed.")
    print("--------------------------------------------------")
