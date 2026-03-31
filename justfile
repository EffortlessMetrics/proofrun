set shell := ["bash", "-cu"]

default:
    @just --list

doctor:
    python3 reference/proofrun_ref.py doctor

plan base head profile="ci":
    python3 reference/proofrun_ref.py plan --base {{base}} --head {{head}} --profile {{profile}}

explain plan=".proofrun/plan.json":
    python3 reference/proofrun_ref.py explain --plan {{plan}}

shell plan=".proofrun/plan.json":
    python3 reference/proofrun_ref.py emit shell --plan {{plan}}

gha plan=".proofrun/plan.json":
    python3 reference/proofrun_ref.py emit github-actions --plan {{plan}}

dry-run plan=".proofrun/plan.json":
    python3 reference/proofrun_ref.py run --plan {{plan}} --dry-run

test:
    python3 -m unittest discover -s tests -p 'test_*.py'
