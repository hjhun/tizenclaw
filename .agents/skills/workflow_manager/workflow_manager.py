#!/usr/bin/env python3
import argparse
import os
import subprocess
import sys
import json
import logging

WORKSPACE_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), "../../../"))
WORKFLOW_DIR = os.path.join(WORKSPACE_DIR, ".agents", "workflows")

def get_latest_mtime(directory, extensions=('.rs', '.cc', '.cpp', '.h', '.hh', '.hpp', '.c')):
    """Gets the latest modification time of any source file in the directory."""
    latest = 0
    for root, dirs, files in os.walk(directory):
        # Exclude target and build directories
        if 'target' in dirs: dirs.remove('target')
        if 'build' in dirs: dirs.remove('build')
        if 'vendor' in dirs: dirs.remove('vendor')
        
        for f in files:
            if any(f.endswith(ext) for ext in extensions):
                mtime = os.path.getmtime(os.path.join(root, f))
                if mtime > latest:
                    latest = mtime
    return latest

def check_deploy_status():
    """Validates that deploy.sh was run after the most recent source code change."""
    deploy_marker = os.path.join(WORKSPACE_DIR, ".deploy_success")
    if not os.path.exists(deploy_marker):
        print("❌ Verification Failed: .deploy_success marker not found.")
        print("   You MUST successfully run deploy.sh before committing.")
        return False
    
    deploy_mtime = os.path.getmtime(deploy_marker)
    src_dir = os.path.join(WORKSPACE_DIR, "src")
    if not os.path.exists(src_dir):
        return True # Probably a non-standard structure, skip
        
    latest_src_mtime = get_latest_mtime(src_dir)
    
    if latest_src_mtime > deploy_mtime:
        print("❌ Verification Failed: Source files have been modified since the last deploy.sh run.")
        print("   You MUST run deploy.sh again to verify your latest changes.")
        return False
        
    print("✅ Deploy verification passed.")
    return True

def run_cargo_check():
    """Enforces the Zero Warnings Policy for Rust code."""
    try:
        if not os.path.exists(os.path.join(WORKSPACE_DIR, "Cargo.toml")):
            return True
            
        print("Running cargo check...")
        result = subprocess.run(
            ["cargo", "check", "--color", "never"], 
            capture_output=True, 
            text=True, 
            cwd=WORKSPACE_DIR
        )
        output = result.stdout + result.stderr
        
        # Check for error or warning
        # Since we might have some allowed warnings globally, we fail strongly on 'warning:' if it persists.
        # Note: The system requires 0 warnings.
        # But we only check lines starting with "warning:" or "error:" to be safe against general output.
        lines = output.split('\n')
        bad_lines = [l for l in lines if l.startswith("warning:") or l.startswith("error:")]
        
        if bad_lines or result.returncode != 0:
            print("❌ Build Verification Failed: Zero Warnings Policy Violation.")
            for l in bad_lines[:10]:
                print(l)
            if len(bad_lines) > 10: print("...")
            print("\n   You MUST fix all Rust warnings and errors before proceeding.")
            return False
            
        print("✅ Cargo build verification passed (0 warnings).")
        return True
    except FileNotFoundError:
        return True

def check_crashes():
    """Checks the connected Tizen device for recent FATAL or SIGABRT logs."""
    try:
        res = subprocess.run(
            ["sdb", "devices"], 
            capture_output=True, text=True, timeout=5
        )
        if "device" not in res.stdout:
            print("⚠️ No sdb device connected. Skipping crash logs verification.")
            return True

        res = subprocess.run(
            ["sdb", "shell", "journalctl", "-u", "tizenclaw", "-n", "50", "--no-pager"], 
            capture_output=True, text=True, timeout=10
        )
        logs = res.stdout + res.stderr
        
        # Look for crash indicators
        bad_indicators = ["SIGABRT", "FATAL", "panic"]
        
        bad_lines = []
        for line in logs.split('\n'):
            line_upper = line.upper()
            if any(ind in line_upper for ind in bad_indicators):
                bad_lines.append(line)
                
        if bad_lines:
            print("❌ Runtime Verification Failed: Detected crash or fatal error in recent device logs.")
            for line in bad_lines:
                print("   >", line)
            print("\n   You MUST resolve this runtime crash before committing.")
            return False
            
        print("✅ Device runtime check passed (No recent crashes).")
        return True
        
    except (FileNotFoundError, PermissionError):
        print("⚠️ sdb not executable or not found in PATH, skipping device crash check.")
        return True
    except subprocess.TimeoutExpired:
        print("⚠️ Timeout waiting for sdb, skipping device crash check.")
        return True

def get_checklist(workflow_name):
    """Retrieves an actionable checklist from a workflow markdown file."""
    if not workflow_name.endswith('.md'):
        workflow_name += '.md'
        
    wf_path = os.path.join(WORKFLOW_DIR, workflow_name)
    if not os.path.exists(wf_path):
        print(f"Error: Workflow '{workflow_name}' not found at {wf_path}.")
        print("Available workflows:")
        for w in os.listdir(WORKFLOW_DIR):
            if w.endswith('.md'):
                print(f" - {w}")
        return False
        
    with open(wf_path, 'r', encoding='utf-8') as f:
        content = f.read()
        
    print(f"--- WORKFLOW: {workflow_name} ---")
    print(content)
    return True

def main():
    parser = argparse.ArgumentParser(description="TizenClaw Workflow Manager Skill")
    parser.add_argument("--action", choices=["verify_status", "get_checklist"], required=True)
    parser.add_argument("--workflow", help="Name of the workflow to retrieve (for get_checklist)")
    
    args = parser.parse_args()
    
    if args.action == "verify_status":
        print("\n=== TIZENCLAW WORKFLOW VERIFICATION ===\n")
        
        if not check_deploy_status():
            sys.exit(1)
            
        if not run_cargo_check():
            sys.exit(1)
            
        if not check_crashes():
            sys.exit(1)
            
        print("\n🎉 All verifications passed successfully! You are allowed to proceed/commit.")
        sys.exit(0)
        
    elif args.action == "get_checklist":
        if not args.workflow:
            print("Error: --workflow argument is required for get_checklist action")
            sys.exit(1)
        if not get_checklist(args.workflow):
            sys.exit(1)

if __name__ == "__main__":
    main()
