use crate::model::Plan;

pub fn emit_plan_markdown(plan: &Plan) -> String {
    crate::explain::render_explanation(plan)
}

pub fn emit_commands_shell(plan: &Plan) -> String {
    let mut out = String::from("#!/usr/bin/env bash\nset -euo pipefail\n\n");
    for surface in &plan.selected_surfaces {
        out.push_str(&format!("# {}\n{}\n\n", surface.id, shell_join(&surface.run)));
    }
    out
}

pub fn emit_github_actions(plan: &Plan) -> String {
    let mut out = String::new();
    out.push_str("steps:\n");
    out.push_str("  - name: Execute proof plan\n");
    out.push_str("    run: |\n");
    for surface in &plan.selected_surfaces {
        out.push_str(&format!("      {}\n", shell_join(&surface.run)));
    }
    out
}

fn shell_join(args: &[String]) -> String {
    args.iter()
        .map(|arg| {
            if arg.chars().all(|ch| ch.is_ascii_alphanumeric() || "/-._:=()".contains(ch)) {
                arg.clone()
            } else {
                format!("'{}'", arg.replace('\'', "'\"'\"'"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
