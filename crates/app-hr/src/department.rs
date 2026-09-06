//! A department, and whether anything may be charged to it.
//!
//! A department and a cost centre are the same thing seen twice, so it is a
//! flag rather than a second table. It is `false` by default: the parent nodes
//! usually are not cost centres, because posting to a division *and* to the
//! cost centres beneath it double-counts.
//!
//! The code is generated (`DEPT-###`, from `core.number_sequences`) rather than
//! typed like a party's, but a workspace migrating from another system may
//! still type one. What is refused is blank. See ADR 0006 section 3.

use phonix_core::Message;
use phonix_core::identity::UserId;
use phonix_core::msg;
use phonix_ports::CostCentre;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Longest code the column holds. Matches `departments_code_format`.
pub const MAX_DEPARTMENT_CODE_LEN: usize = 40;

/// Longest name the column holds. Matches `departments_name_present`.
pub const MAX_DEPARTMENT_NAME_LEN: usize = 120;

/// How deep the tree may go. Checked when a parent is chosen — a `CHECK`
/// cannot walk a tree. Four levels is already a large organization.
pub const MAX_DEPARTMENT_DEPTH: usize = 8;

/// One department, as a screen and a port see it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Department {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub is_cost_centre: bool,
    pub manager_user_id: Option<UserId>,
    pub is_active: bool,
}

/// One row of the tree, with what it takes to draw it. `depth` and
/// `manager_name` are derived by the reader, not stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DepartmentSummary {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub is_cost_centre: bool,
    pub is_active: bool,
    /// 0 for a root. Drives the indentation in the grid.
    pub depth: u16,
    /// Who runs it, resolved for display. `None` where nobody does, and also
    /// where the manager's account has been deleted.
    pub manager_name: Option<String>,
    /// Carried on the row so the delete button does not need a query per row.
    pub child_count: i64,
}

impl Department {
    /// The snapshot a document keeps when it charges something here.
    ///
    /// `None` for a grouping or a retired row. The only place a department
    /// becomes a [`CostCentre`], so the port and the query cannot disagree
    /// about what qualifies.
    pub fn as_cost_centre(&self) -> Option<CostCentre> {
        (self.is_cost_centre && self.is_active).then(|| CostCentre {
            id: self.id,
            code: self.code.clone(),
            name: self.name.clone(),
        })
    }
}

/// The editable part of a department.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DepartmentInput {
    /// Absent means create, so a screen cannot open the create form and submit
    /// it against an existing row.
    pub id: Option<Uuid>,
    /// Empty on create means "allocate one"; typed means use it as typed.
    pub code: String,
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub is_cost_centre: bool,
    pub manager_user_id: Option<UserId>,
    pub is_active: bool,
}

impl DepartmentInput {
    pub fn blank() -> Self {
        Self {
            id: None,
            code: String::new(),
            name: String::new(),
            parent_id: None,
            is_cost_centre: false,
            manager_user_id: None,
            is_active: true,
        }
    }

    /// A blank form for a child of something, so "add a sub-department" opens
    /// with the parent already chosen.
    pub fn under(parent_id: Uuid) -> Self {
        Self {
            parent_id: Some(parent_id),
            ..Self::blank()
        }
    }

    pub fn from_department(department: &Department) -> Self {
        Self {
            id: Some(department.id),
            code: department.code.clone(),
            name: department.name.clone(),
            parent_id: department.parent_id,
            is_cost_centre: department.is_cost_centre,
            manager_user_id: department.manager_user_id,
            is_active: department.is_active,
        }
    }

    /// Trim, and say what is still wrong.
    ///
    /// An empty code is allowed on create — it means "allocate one" — and
    /// refused on edit, where a saved row already has one.
    pub fn check(&self) -> Result<Self, DepartmentError> {
        let code = self.code.trim();
        let name = self.name.trim();

        if code.is_empty() && self.id.is_some() {
            return Err(DepartmentError::CodeRequired);
        }
        if !code.is_empty() {
            if code.chars().count() > MAX_DEPARTMENT_CODE_LEN {
                return Err(DepartmentError::CodeTooLong);
            }
            // Matches `departments_code_format`.
            if !code.starts_with(|c: char| c.is_ascii_alphanumeric())
                || !code
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
            {
                return Err(DepartmentError::CodeShape);
            }
        }

        if name.is_empty() {
            return Err(DepartmentError::NameRequired);
        }
        if name.chars().count() > MAX_DEPARTMENT_NAME_LEN {
            return Err(DepartmentError::NameTooLong);
        }

        // Also a CHECK; caught here so the form names the field.
        if let (Some(id), Some(parent_id)) = (self.id, self.parent_id)
            && id == parent_id
        {
            return Err(DepartmentError::OwnParent);
        }

        Ok(Self {
            id: self.id,
            code: code.to_owned(),
            name: name.to_owned(),
            parent_id: self.parent_id,
            is_cost_centre: self.is_cost_centre,
            manager_user_id: self.manager_user_id,
            is_active: self.is_active,
        })
    }
}

/// What can be wrong with a department somebody typed.
///
/// [`Self::Cycle`] and [`Self::TooDeep`] need the rest of the tree, so the
/// service raises them; they live here because a screen renders all of these
/// the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DepartmentError {
    #[error("a department needs a code")]
    CodeRequired,
    #[error("a department code is at most 40 characters")]
    CodeTooLong,
    #[error("a department code may contain only letters, digits, hyphens and underscores")]
    CodeShape,
    #[error("a department needs a name")]
    NameRequired,
    #[error("a department name is at most 120 characters")]
    NameTooLong,
    #[error("a department cannot be its own parent")]
    OwnParent,
    #[error("that would put a department underneath itself")]
    Cycle,
    #[error("departments are at most eight levels deep")]
    TooDeep,
}

impl DepartmentError {
    /// Which control to attach the message to.
    pub fn field(self) -> &'static str {
        match self {
            Self::CodeRequired | Self::CodeTooLong | Self::CodeShape => "code",
            Self::NameRequired | Self::NameTooLong => "name",
            Self::OwnParent | Self::Cycle | Self::TooDeep => "parent_id",
        }
    }

    pub fn message(self) -> Message {
        match self {
            Self::CodeRequired => msg!("department.error.code_required"),
            Self::CodeTooLong => msg!("department.error.code_too_long"),
            Self::CodeShape => msg!("department.error.code_shape"),
            Self::NameRequired => msg!("department.error.name_required"),
            Self::NameTooLong => msg!("department.error.name_too_long"),
            Self::OwnParent => msg!("department.error.own_parent"),
            Self::Cycle => msg!("department.error.cycle"),
            Self::TooDeep => msg!("department.error.too_deep"),
        }
    }
}

/// What a delete answered. A value rather than a `Result`: two of the three are
/// things a screen renders beside the button, not faults.
///
/// Here rather than in `phonix-services` because it crosses the wire as a
/// server function's return type, so the wasm client has to know it too.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeleteOutcome {
    /// Gone. Also the answer when it was already gone.
    Deleted,
    /// Departments sit inside it. Names how many.
    HasChildren { count: i64 },
    /// A cost centre: something may have been charged here, and this schema
    /// cannot see whether it was.
    MayBeInUse,
}

/// Order a flat list into the tree and say how deep each row is: roots first,
/// then each row's children immediately beneath it. The grid indents by
/// `depth`.
///
/// A row whose parent is not in `rows` is treated as a root rather than
/// dropped — that is the normal case under a filter, and a screen that silently
/// loses rows is how nobody finds out.
pub fn in_tree_order(rows: Vec<DepartmentSummary>) -> Vec<DepartmentSummary> {
    let present: std::collections::HashSet<Uuid> = rows.iter().map(|row| row.id).collect();

    // Index by parent once; scanning per node would be quadratic.
    let mut children: std::collections::HashMap<Option<Uuid>, Vec<DepartmentSummary>> =
        std::collections::HashMap::new();
    for row in rows {
        let parent = row.parent_id.filter(|id| present.contains(id));
        children.entry(parent).or_default().push(row);
    }

    let mut ordered = Vec::new();

    // The rows themselves, not their ids: a node is emitted the moment it is
    // popped and its children go on top, which is what puts a child
    // immediately under its parent rather than under the last of its aunts.
    let mut stack: Vec<(DepartmentSummary, u16)> = Vec::new();

    // Reversed on the way in, so the last pushed is the first popped and
    // siblings come out in the order they arrived.
    if let Some(mut roots) = children.remove(&None) {
        roots.reverse();
        stack.extend(roots.into_iter().map(|row| (row, 0)));
    }

    // Iterative: this is handed whatever the database returned, and a stack
    // overflow in the wasm bundle takes the whole page down.
    while let Some((mut row, depth)) = stack.pop() {
        row.depth = depth;
        let id = row.id;
        ordered.push(row);

        if let Some(mut batch) = children.remove(&Some(id)) {
            batch.reverse();
            let depth = depth.saturating_add(1);
            stack.extend(batch.into_iter().map(|row| (row, depth)));
        }
    }

    // Unreachable from any root means a cycle in stored data. Shown, not lost.
    for (_, batch) in children {
        ordered.extend(batch);
    }

    ordered
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> DepartmentInput {
        DepartmentInput {
            name: "Finance".to_owned(),
            ..DepartmentInput::blank()
        }
    }

    fn summary(id: u8, parent: Option<u8>, name: &str) -> DepartmentSummary {
        DepartmentSummary {
            id: Uuid::from_bytes([id; 16]),
            code: format!("DEPT-{id:03}"),
            name: name.to_owned(),
            parent_id: parent.map(|p| Uuid::from_bytes([p; 16])),
            is_cost_centre: false,
            is_active: true,
            depth: 0,
            manager_name: None,
            child_count: 0,
        }
    }

    #[test]
    fn a_new_department_may_arrive_without_a_code() {
        let checked = input().check().expect("a blank code is allowed on create");
        assert_eq!(checked.code, "");
    }

    #[test]
    fn an_existing_department_may_not_lose_its_code() {
        let existing = DepartmentInput {
            id: Some(Uuid::from_bytes([1; 16])),
            ..input()
        };
        assert_eq!(existing.check(), Err(DepartmentError::CodeRequired));
    }

    #[test]
    fn a_typed_code_is_kept_and_checked() {
        let typed = DepartmentInput {
            code: "  FIN-01  ".to_owned(),
            ..input()
        };
        assert_eq!(typed.check().expect("valid").code, "FIN-01");

        let spaced = DepartmentInput {
            code: "FIN 01".to_owned(),
            ..input()
        };
        assert_eq!(spaced.check(), Err(DepartmentError::CodeShape));

        // `-01` reads as a negative number in a spreadsheet.
        let leading = DepartmentInput {
            code: "-FIN".to_owned(),
            ..input()
        };
        assert_eq!(leading.check(), Err(DepartmentError::CodeShape));
    }

    #[test]
    fn a_department_needs_a_name() {
        let blank = DepartmentInput {
            name: "   ".to_owned(),
            ..input()
        };
        assert_eq!(blank.check(), Err(DepartmentError::NameRequired));
    }

    #[test]
    fn a_department_is_not_its_own_parent() {
        let id = Uuid::from_bytes([7; 16]);
        let looped = DepartmentInput {
            id: Some(id),
            code: "FIN".to_owned(),
            parent_id: Some(id),
            ..input()
        };
        assert_eq!(looped.check(), Err(DepartmentError::OwnParent));
    }

    #[test]
    fn only_an_active_cost_centre_can_be_charged_to() {
        let mut department = Department {
            id: Uuid::from_bytes([3; 16]),
            code: "DEPT-003".to_owned(),
            name: "Finance".to_owned(),
            parent_id: None,
            is_cost_centre: true,
            manager_user_id: None,
            is_active: true,
        };

        assert!(department.as_cost_centre().is_some());

        // A grouping: nothing posts to it.
        department.is_cost_centre = false;
        assert!(department.as_cost_centre().is_none());

        department.is_cost_centre = true;
        department.is_active = false;
        assert!(department.as_cost_centre().is_none());
    }

    #[test]
    fn the_tree_comes_out_depth_first_with_siblings_in_order() {
        let ordered = in_tree_order(vec![
            summary(1, None, "Operations"),
            summary(2, Some(1), "Warehouse"),
            summary(3, Some(1), "Logistics"),
            summary(4, Some(2), "Goods in"),
            summary(5, None, "Finance"),
        ]);

        let seen: Vec<(&str, u16)> = ordered
            .iter()
            .map(|row| (row.name.as_str(), row.depth))
            .collect();

        assert_eq!(
            seen,
            vec![
                ("Operations", 0),
                ("Warehouse", 1),
                ("Goods in", 2),
                ("Logistics", 1),
                ("Finance", 0),
            ]
        );
    }

    #[test]
    fn a_row_whose_parent_was_filtered_out_is_still_shown() {
        // What the cost-centre filter does constantly.
        let ordered = in_tree_order(vec![summary(2, Some(1), "Warehouse")]);

        assert_eq!(ordered.len(), 1);
        assert_eq!(ordered[0].depth, 0);
    }

    #[test]
    fn a_cycle_in_stored_data_is_shown_rather_than_lost() {
        let mut a = summary(1, Some(2), "A");
        let b = summary(2, Some(1), "B");
        a.parent_id = Some(b.id);

        let ordered = in_tree_order(vec![a, b]);

        // Neither is reachable from a root; both come back.
        assert_eq!(ordered.len(), 2);
    }
}
