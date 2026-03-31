use crate::model::Plan;

pub fn render_explanation(plan: &Plan) -> String {
    let mut out = String::new();
    out.push_str("# proofrun plan\n\n");
    out.push_str(&format!("range: {}..{}\n\n", plan.base, plan.head));
    out.push_str("selected surfaces:\n");
    for surface in &plan.selected_surfaces {
        out.push_str(&format!(
            "- {} (cost {}) -> {}\n",
            surface.id,
            surface.cost,
            surface.covers.join(", ")
        ));
    }
    out
}
