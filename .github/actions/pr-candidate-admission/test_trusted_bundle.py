"""Two-tree controls for the production routed admission executable selector."""
import base64
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = (Path(sys.argv[1]).resolve() if sys.argv[1:] != ["--self-test"]
        else Path(__file__).resolve().parents[3])
sys.argv = sys.argv[:1]
ACTION = ".github/actions/pr-candidate-admission"
FILES = ("action.yml", "candidate_admission.py", "release_cut_admission.py")
PIN = "${{ steps.trusted.outputs.sha }}"


def bash():
    """The bash the workflow steps run under. On Windows a bare `bash` resolves to
    the WSL launcher in System32 (no distributions installed on the runner), so
    prefer Git for Windows' bash and ignore PATH, which a case may poison."""
    if os.name != "nt":
        return "bash"
    for variable in ("ProgramFiles", "ProgramFiles(x86)"):
        prefix = os.environ.get(variable)
        for relative in ("Git/bin/bash.exe", "Git/usr/bin/bash.exe"):
            if prefix and (Path(prefix) / relative).is_file():
                return str(Path(prefix) / relative)
    raise RuntimeError("Git for Windows bash.exe not found under ProgramFiles")


def shell(source):
    return "\n".join(line[8:] if line.startswith("        ") else line
                     for line in source.split("      run: |\n", 1)[1].splitlines())


def script_argv(source, directory):
    """Run a workflow step from a file. Passing the multi-line script as one
    `bash -c` argument is re-parsed by the Windows/MSYS command line and loses
    quoting, so the long action step failed with `unexpected end of file`."""
    script = Path(directory) / "workflow-step.sh"
    script.write_bytes(source.encode("utf-8") + b"\n")  # LF: CRLF breaks bash
    return [bash(), "-euo", "pipefail", script.as_posix()]


def step(workflow, name):
    marker = f"      - name: {name}\n"
    if marker not in workflow:
        return None
    return workflow.split(marker, 1)[1].split("\n      - ", 1)[0]


GH = r'''#!/usr/bin/env python3
import base64,json,os,sys
from pathlib import Path
d=json.loads(Path(os.environ['CONTROL_CASE']).read_text(encoding="utf-8"))
url=next(x for x in sys.argv if x.startswith('repos/'))
head='a'*40; base='d'*40
if '/git/ref/' in url: value=base
elif '/git/commits/' in url:
    sha=url.rsplit('/',1)[1]
    value={'sha':sha,'tree':{'sha':'c'*40 if sha=='b'*40 else 'e'*40},'parents':[] if sha=='b'*40 else [{'sha':'b'*40}]}
elif '/files?' in url:
    value=[{'filename':d.get('filename','src/lib.rs'),'status':'modified','additions':d.get('lines',1),'deletions':0}]
elif '/pulls?' in url: value=[]
elif '/pulls/' in url:
    value={'number':10,'state':'open','draft':False,'head':{'sha':head,'repo':{'full_name':'o/r'}},'base':{'ref':'main','sha':base},'changed_files':1,'additions':d.get('lines',1),'deletions':0,'body':'Governing issue or accepted claim: #4986\nAcceptance delta (one sentence): trusted bundle controls'}
elif '/contents/.gitattributes?' in url:
    print(d.get('base_attributes',''));sys.exit(0)
elif '/contents/policy/release-hard-cut-admission.json?' in url:
    packet=d['packet'] if url.endswith(base) else d['inactive']
    value={'content':base64.b64encode(json.dumps(packet).encode()).decode()}
else: raise SystemExit('unexpected read: '+url)
if '--jq' in sys.argv:
    query=sys.argv[sys.argv.index('--jq')+1]
    if query=='.head.sha':value=value['head']['sha']
    elif query=='.object.sha // empty':pass
    else:raise SystemExit('unexpected jq: '+query)
print(value if isinstance(value,str) else json.dumps(value))
'''


class Bundle(unittest.TestCase):
    def run_case(self, *, damage="", poison="wrapper", lines=1, active=False,
                 attributes="", filename="src/lib.rs", base_attributes="", mutant=False):
        with tempfile.TemporaryDirectory(prefix="ripr-trusted-bundle-") as temporary:
            tmp = Path(temporary)
            workspace = tmp / "candidate"
            candidate = workspace / ACTION
            candidate.mkdir(parents=True)
            (workspace / ".gitattributes").write_text(attributes, encoding="utf-8")
            (workspace / "policy").mkdir()
            inactive = json.loads((ROOT / "policy/release-hard-cut-admission.json").read_text(encoding="utf-8"))
            (workspace / "policy/release-hard-cut-admission.json").write_text(json.dumps(inactive), encoding="utf-8")
            trusted = workspace / ".admission-trusted"
            trusted_action = trusted / ACTION
            trusted_action.mkdir(parents=True)
            for name in FILES:
                content = (ROOT / ACTION / name).read_bytes()
                (trusted_action / name).write_bytes(content)
                (candidate / name).write_bytes(content)
            marker = tmp / "candidate-executed"
            if poison == "wrapper":
                (candidate / "action.yml").write_text('name: Candidate poison\ndescription: Controlled candidate wrapper fixture\nruns:\n  using: composite\n  steps:\n    - shell: bash\n      run: |\n        touch "$CONTROL_MARKER"\n        exit 0\n', encoding="utf-8")
            else:
                for name in FILES[1:]:
                    (candidate / name).write_text("import os,pathlib\npathlib.Path(os.environ['CONTROL_MARKER']).touch()\n", encoding="utf-8")
            env = dict(os.environ, GITHUB_WORKSPACE=str(workspace), RUNNER_TEMP=str(tmp), CONTROL_MARKER=str(marker),
                       GITHUB_OUTPUT=str(tmp / "outputs"), GITHUB_STEP_SUMMARY=str(tmp / "summary"))
            def run(*args, **kwargs):
                return subprocess.run(args, env=env, text=True, encoding="utf-8", stdout=subprocess.PIPE,
                                      stderr=subprocess.PIPE, check=False, **kwargs)
            for args in [("git", "init", "-q", str(trusted)),
                         ("git", "-C", str(trusted), "add", ACTION),
                         ("git", "-C", str(trusted), "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "-c", "commit.gpgsign=false", "commit", "-qm", "trusted fixture")]:
                result = run(*args)
                self.assertEqual(result.returncode, 0, result.stderr)
            expected = run("git", "-C", str(trusted), "rev-parse", "HEAD").stdout.strip()
            env["TRUSTED_ADMISSION_SHA"] = expected
            if damage == "missing":
                (trusted_action / "release_cut_admission.py").unlink()
            elif damage == "mismatch":
                (trusted_action / "candidate_admission.py").write_text("raise SystemExit(0)\n", encoding="utf-8")
            elif damage == "head":
                env["TRUSTED_ADMISSION_SHA"] = "0" * 40
            workflow = (ROOT / ".github/workflows/routed-rust.yml").read_text(encoding="utf-8")
            verify = step(workflow, "Verify trusted admission bundle")
            if verify:
                result = run(*script_argv(shell(verify), tmp), cwd=workspace)
                if result.returncode:
                    self.assertFalse(marker.exists())
                    return result.returncode, result.stdout + result.stderr, False
            entry = step(workflow, "Admit exact candidate")
            selector = next(line.strip().removeprefix("uses: ./") for line in entry.splitlines()
                            if line.strip().startswith("uses: ./"))
            if mutant:
                selector = ACTION
            action = workspace / selector
            env.update(GITHUB_ACTION_PATH=str(action), GH_TOKEN="fixture", ADMISSION_REPOSITORY="o/r",
                       ADMISSION_PR_NUMBER="10", ADMISSION_EXPECTED_HEAD="a" * 40,
                       ADMISSION_DOCS_ONLY="false", ADMISSION_MAX_CHANGED_FILES="160",
                       ADMISSION_MAX_CHANGED_LINES="3000", ADMISSION_MODE="admit")
            packet = dict(inactive)
            if active:
                packet.update(state="active", generation="cut-1", observed_main={"sha":"b"*40,"tree":"c"*40},
                              admitted=[{"pr":9,"head_sha":"9"*40,"admission":"admitted","disposition":"release_required_product","owner_issue":99,"release_effect":"landed before cut","proof_refs":["#99"]}])
            case = tmp / "case.json"
            case.write_text(json.dumps(dict(lines=lines, packet=packet, inactive=inactive,
                                           filename=filename, base_attributes=base_attributes)), encoding="utf-8")
            env["CONTROL_CASE"] = str(case)
            (tmp / "gh").write_bytes(GH.encode("utf-8"))  # LF: a CRLF shebang cannot run
            (tmp / "gh").chmod(0o755)
            (tmp / "gh.cmd").write_text(f'@"{sys.executable}" "%~dp0gh" %*\r\n', encoding="utf-8")
            env["PATH"] = str(tmp) + os.pathsep + env["PATH"]
            result = run(*script_argv(shell((action / "action.yml").read_text(encoding="utf-8")), tmp), cwd=workspace)
            outputs = (tmp / "outputs").read_text(encoding="utf-8") if (tmp / "outputs").exists() else ""
            return result.returncode, outputs + result.stdout + result.stderr, marker.exists()

    def test_candidate_wrapper_and_both_interpreters_are_data(self):
        for poison in ("wrapper", "interpreters"):
            with self.subTest(poison=poison):
                code, output, executed = self.run_case(poison=poison, lines=3000)
                self.assertFalse(executed, "candidate executable authority was used")
                self.assertEqual(code, 0, output)
                self.assertIn("status=admitted", output)

    def test_oversized_and_active_cut_still_refuse(self):
        for options, reason in [(dict(lines=3001), "qualification_scope_oversized"),
                                (dict(active=True), "cut_frozen")]:
            code, output, executed = self.run_case(**options)
            self.assertFalse(executed)
            self.assertEqual(code, 1, output)
            self.assertIn(reason, output)

    def test_missing_modified_or_wrong_source_refuses(self):
        for damage in ("missing", "mismatch", "head"):
            with self.subTest(damage=damage):
                code, output, executed = self.run_case(damage=damage)
                self.assertFalse(executed)
                self.assertNotEqual(code, 0, output)
                self.assertIn("trusted admission", output)

    def test_candidate_attributes_are_checked_and_exemptions_are_base_owned(self):
        code, output, _ = self.run_case(attributes="src/*.rs linguist-generated\n")
        self.assertEqual(code, 1, output)
        self.assertIn("generated_declaration_invalid", output)
        declaration = "package-lock.json linguist-generated\n"
        code, output, _ = self.run_case(lines=3001, filename="package-lock.json", attributes=declaration)
        self.assertEqual(code, 1, output)
        self.assertIn("qualification_scope_oversized", output)
        code, output, _ = self.run_case(lines=3001, filename="package-lock.json", attributes=declaration,
                                        base_attributes=declaration)
        self.assertEqual(code, 0, output)
        self.assertIn("changed_lines=0", output)

    def test_wrong_selector_mutant_exposes_candidate_execution(self):
        code, output, executed = self.run_case(mutant=True)
        self.assertTrue(executed, "mutant did not reach the poisoned candidate wrapper")
        self.assertEqual(code, 0, output)

    def test_caller_pins_both_action_invocations_and_checks_reserved_destination(self):
        workflow = (ROOT / ".github/workflows/routed-rust.yml").read_text(encoding="utf-8")
        admission = workflow.split("  admission:\n", 1)[1].split("\n  rust-cx43:", 1)[0]
        self.assertIn("ref: " + PIN, admission)
        self.assertIn("TRUSTED_ADMISSION_SHA: " + PIN, admission)
        self.assertEqual(admission.count("uses: ./.admission-trusted/" + ACTION), 2)
        check = step(workflow, "Select trusted admission source")
        self.assertIsNotNone(check)
        with tempfile.TemporaryDirectory(prefix="ripr-admission-collision-") as temporary:
            workspace = Path(temporary)
            for collision in (False, True):
                if collision:
                    (workspace / ".admission-trusted").mkdir()
                result = subprocess.run([bash(), "-euo", "pipefail", "-c", shell(check)], cwd=workspace,
                                        env=dict(os.environ, GITHUB_WORKSPACE=str(workspace),
                                                 GITHUB_OUTPUT=str(workspace / "outputs"), EVENT_NAME="pull_request",
                                                 PR_BASE_SHA="b"*40, GROUP_BASE_SHA="c"*40, EVENT_SHA="a"*40), capture_output=True)
                self.assertEqual(result.returncode, int(collision), result)

    def test_event_identity_selection_has_no_candidate_fallback(self):
        check = step((ROOT / ".github/workflows/routed-rust.yml").read_text(encoding="utf-8"), "Select trusted admission source")
        self.assertIsNotNone(check)
        with tempfile.TemporaryDirectory(prefix="ripr-admission-source-") as temporary:
            output = Path(temporary) / "outputs"
            for event, field, expected in [("pull_request", "PR_BASE_SHA", "b"),
                                            ("merge_group", "GROUP_BASE_SHA", "c"),
                                            ("push", "EVENT_SHA", "a"),
                                            ("workflow_dispatch", "EVENT_SHA", "a")]:
                for missing in (False, True):
                    with self.subTest(event=event, missing=missing):
                        env = dict(os.environ, GITHUB_WORKSPACE=temporary, GITHUB_OUTPUT=str(output),
                                   EVENT_NAME=event, PR_BASE_SHA="b"*40, GROUP_BASE_SHA="c"*40, EVENT_SHA="a"*40)
                        if missing:
                            env[field] = ""
                        output.write_text("", encoding="utf-8")
                        result = subprocess.run([bash(), "-euo", "pipefail", "-c", shell(check)],
                                                env=env, capture_output=True)
                        self.assertEqual(result.returncode, int(missing), result)
                        self.assertEqual(output.read_text(encoding="utf-8"), "" if missing else "sha="+expected*40+"\n")


if __name__ == "__main__":
    result = unittest.TextTestRunner().run(unittest.defaultTestLoader.loadTestsFromTestCase(Bundle))
    if not result.wasSuccessful():
        raise SystemExit(1)
    print("trusted admission bundle controls passed")
