//! SQLite-backed adapter for the hosting-plans bounded context.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::{
    HostingPlan, HostingPlanId, HostingPlanRepository, PlanAssignment, PlanId, PlanQuotas,
    PlanStatus, RepoError, hosting_plans::HostingPlansError,
};
use sqlx::{Pool, Sqlite};
use uuid::Uuid;

/// SQLite-backed repository for hosting plans and assignments.
#[derive(Clone)]
pub struct SqliteHostingPlanRepository {
    pool: Pool<Sqlite>,
}

impl SqliteHostingPlanRepository {
    /// Build a repository over the given SQLite pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HostingPlanRepository for SqliteHostingPlanRepository {
    async fn insert(&self, plan: &HostingPlan) -> Result<(), HostingPlansError> {
        let caps_json = serde_json::to_string(plan.quota_caps())
            .map_err(|e| HostingPlansError::Persistence(e.to_string()))?;
        let res = sqlx::query(
            r#"
            INSERT INTO hosting_plans
                (id, name, description, prices_json, features_json,
                 quota_caps_json, allowed_apps_json, allowed_php_runtimes_json,
                 status, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(plan.id().as_uuid().to_string())
        .bind(plan.name())
        .bind(plan.description())
        .bind(plan.prices_json())
        .bind(plan.features_json())
        .bind(caps_json)
        .bind(plan.allowed_apps_json())
        .bind(plan.allowed_php_runtimes_json())
        .bind(plan_status_str(plan.status()))
        .bind(plan.created_at().to_rfc3339())
        .bind(plan.updated_at().to_rfc3339())
        .execute(&self.pool)
        .await;
        match res {
            Ok(_) => Ok(()),
            Err(e) if is_unique_violation(&e) => Err(HostingPlansError::DuplicateName),
            Err(e) => Err(HostingPlansError::Persistence(e.to_string())),
        }
    }

    async fn find_by_id(&self, id: PlanId) -> Result<Option<HostingPlan>, HostingPlansError> {
        let row: Option<PlanRow> = sqlx::query_as::<_, PlanRow>(
            "SELECT id, name, description, prices_json, features_json, quota_caps_json, allowed_apps_json, allowed_php_runtimes_json, status, created_at, updated_at FROM hosting_plans WHERE id = ?",
        )
        .bind(id.as_uuid().to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| HostingPlansError::Persistence(e.to_string()))?;
        row.map(PlanRow::into_plan).transpose()
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<HostingPlan>, HostingPlansError> {
        let row: Option<PlanRow> = sqlx::query_as::<_, PlanRow>(
            "SELECT id, name, description, prices_json, features_json, quota_caps_json, allowed_apps_json, allowed_php_runtimes_json, status, created_at, updated_at FROM hosting_plans WHERE name = ?",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| HostingPlansError::Persistence(e.to_string()))?;
        row.map(PlanRow::into_plan).transpose()
    }

    async fn list(&self) -> Result<Vec<HostingPlan>, HostingPlansError> {
        let rows: Vec<PlanRow> = sqlx::query_as::<_, PlanRow>(
            "SELECT id, name, description, prices_json, features_json, quota_caps_json, allowed_apps_json, allowed_php_runtimes_json, status, created_at, updated_at FROM hosting_plans ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| HostingPlansError::Persistence(e.to_string()))?;
        rows.into_iter().map(PlanRow::into_plan).collect()
    }

    async fn update(&self, plan: &HostingPlan) -> Result<(), HostingPlansError> {
        let caps_json = serde_json::to_string(plan.quota_caps())
            .map_err(|e| HostingPlansError::Persistence(e.to_string()))?;
        let res = sqlx::query(
            r#"
            UPDATE hosting_plans
            SET name = ?, description = ?, prices_json = ?, features_json = ?,
                quota_caps_json = ?, allowed_apps_json = ?,
                allowed_php_runtimes_json = ?, status = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(plan.name())
        .bind(plan.description())
        .bind(plan.prices_json())
        .bind(plan.features_json())
        .bind(caps_json)
        .bind(plan.allowed_apps_json())
        .bind(plan.allowed_php_runtimes_json())
        .bind(plan_status_str(plan.status()))
        .bind(plan.updated_at().to_rfc3339())
        .bind(plan.id().as_uuid().to_string())
        .execute(&self.pool)
        .await;
        match res {
            Ok(_) => Ok(()),
            Err(e) if is_unique_violation(&e) => Err(HostingPlansError::DuplicateName),
            Err(e) => Err(HostingPlansError::Persistence(e.to_string())),
        }
    }

    async fn delete(&self, id: PlanId) -> Result<(), HostingPlansError> {
        let count = self.count_assignments(id).await?;
        if count > 0 {
            return Err(HostingPlansError::PlanInUse);
        }
        sqlx::query("DELETE FROM hosting_plans WHERE id = ?")
            .bind(id.as_uuid().to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| HostingPlansError::Persistence(e.to_string()))?;
        Ok(())
    }

    async fn insert_assignment(
        &self,
        assignment: &PlanAssignment,
    ) -> Result<(), HostingPlansError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| HostingPlansError::Persistence(e.to_string()))?;
        sqlx::query(
            "INSERT INTO user_plan_assignments (user_id, plan_id, assigned_at, assigned_by, replaced_at) VALUES (?, ?, ?, ?, NULL)",
        )
        .bind(assignment.user_id().to_string())
        .bind(assignment.plan_id().as_uuid().to_string())
        .bind(assignment.assigned_at().to_rfc3339())
        .bind(assignment.assigned_by().to_string())
        .execute(&mut *tx)
        .await
        .map_err(|e| HostingPlansError::Persistence(e.to_string()))?;
        tx.commit()
            .await
            .map_err(|e| HostingPlansError::Persistence(e.to_string()))?;
        Ok(())
    }

    async fn replace_current_assignment(
        &self,
        user_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Option<PlanId>, HostingPlansError> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT plan_id FROM user_plan_assignments WHERE user_id = ? AND replaced_at IS NULL ORDER BY assigned_at DESC LIMIT 1",
        )
        .bind(user_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| HostingPlansError::Persistence(e.to_string()))?;
        let prev = match row {
            Some((plan_id,)) => Some(
                Uuid::parse_str(&plan_id)
                    .map_err(|e| HostingPlansError::Persistence(e.to_string()))?,
            ),
            None => None,
        };
        sqlx::query("UPDATE user_plan_assignments SET replaced_at = ? WHERE user_id = ? AND replaced_at IS NULL")
            .bind(now.to_rfc3339())
            .bind(user_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| HostingPlansError::Persistence(e.to_string()))?;
        Ok(prev.map(HostingPlanId))
    }

    async fn remove_current_assignment(
        &self,
        user_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Option<PlanId>, HostingPlansError> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT plan_id FROM user_plan_assignments WHERE user_id = ? AND replaced_at IS NULL ORDER BY assigned_at DESC LIMIT 1",
        )
        .bind(user_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| HostingPlansError::Persistence(e.to_string()))?;
        let prev = match row {
            Some((plan_id,)) => Some(
                Uuid::parse_str(&plan_id)
                    .map_err(|e| HostingPlansError::Persistence(e.to_string()))?,
            ),
            None => None,
        };
        sqlx::query("UPDATE user_plan_assignments SET replaced_at = ? WHERE user_id = ? AND replaced_at IS NULL")
            .bind(now.to_rfc3339())
            .bind(user_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| HostingPlansError::Persistence(e.to_string()))?;
        Ok(prev.map(HostingPlanId))
    }

    async fn current_assignment(
        &self,
        user_id: Uuid,
    ) -> Result<Option<PlanAssignment>, HostingPlansError> {
        let row: Option<AssignmentRow> = sqlx::query_as::<_, AssignmentRow>(
            "SELECT user_id, plan_id, assigned_at, assigned_by, replaced_at FROM user_plan_assignments WHERE user_id = ? AND replaced_at IS NULL ORDER BY assigned_at DESC LIMIT 1",
        )
        .bind(user_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| HostingPlansError::Persistence(e.to_string()))?;
        row.map(AssignmentRow::into_assignment).transpose()
    }

    async fn count_assignments(&self, plan_id: PlanId) -> Result<i64, HostingPlansError> {
        let n = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM user_plan_assignments WHERE plan_id = ? AND replaced_at IS NULL",
        )
        .bind(plan_id.as_uuid().to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| HostingPlansError::Persistence(e.to_string()))?;
        Ok(n)
    }

    async fn exists(&self, id: PlanId) -> Result<bool, RepoError> {
        let n = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM hosting_plans WHERE id = ?")
            .bind(id.as_uuid().to_string())
            .fetch_one(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(n > 0)
    }
}

fn plan_status_str(status: PlanStatus) -> &'static str {
    match status {
        PlanStatus::Active => "active",
        PlanStatus::Disabled => "disabled",
    }
}

fn parse_plan_status(raw: &str) -> Result<PlanStatus, HostingPlansError> {
    match raw {
        "active" => Ok(PlanStatus::Active),
        "disabled" => Ok(PlanStatus::Disabled),
        other => Err(HostingPlansError::Persistence(format!(
            "unknown plan status `{other}`"
        ))),
    }
}

fn is_unique_violation(err: &sqlx::Error) -> bool {
    matches!(err, sqlx::Error::Database(db) if db.code().as_deref() == Some("2067") || db.message().contains("UNIQUE"))
}

#[derive(sqlx::FromRow)]
struct PlanRow {
    id: String,
    name: String,
    description: String,
    prices_json: String,
    features_json: String,
    quota_caps_json: String,
    allowed_apps_json: String,
    allowed_php_runtimes_json: String,
    status: String,
    created_at: String,
    updated_at: String,
}

impl PlanRow {
    fn into_plan(self) -> Result<HostingPlan, HostingPlansError> {
        let id = Uuid::parse_str(&self.id)
            .map_err(|e| HostingPlansError::Persistence(format!("bad plan id: {e}")))?;
        let quota_caps: PlanQuotas = serde_json::from_str(&self.quota_caps_json)
            .map_err(|e| HostingPlansError::Persistence(e.to_string()))?;
        let status = parse_plan_status(&self.status)?;
        let created_at = parse_dt(&self.created_at)?;
        let updated_at = parse_dt(&self.updated_at)?;
        HostingPlan::restore(
            HostingPlanId(id),
            self.name,
            self.description,
            &self.prices_json,
            &self.features_json,
            quota_caps,
            &self.allowed_apps_json,
            &self.allowed_php_runtimes_json,
            status,
            created_at,
            updated_at,
        )
    }
}

#[derive(sqlx::FromRow)]
struct AssignmentRow {
    user_id: String,
    plan_id: String,
    assigned_at: String,
    assigned_by: String,
    replaced_at: Option<String>,
}

impl AssignmentRow {
    fn into_assignment(self) -> Result<PlanAssignment, HostingPlansError> {
        let user_id = Uuid::parse_str(&self.user_id)
            .map_err(|e| HostingPlansError::Persistence(format!("bad user id: {e}")))?;
        let plan_id = Uuid::parse_str(&self.plan_id)
            .map_err(|e| HostingPlansError::Persistence(format!("bad plan id: {e}")))?;
        let assigned_by = Uuid::parse_str(&self.assigned_by)
            .map_err(|e| HostingPlansError::Persistence(format!("bad assigned_by: {e}")))?;
        let assigned_at = parse_dt(&self.assigned_at)?;
        let replaced_at = self.replaced_at.as_deref().map(parse_dt).transpose()?;
        Ok(PlanAssignment::restore(
            user_id,
            HostingPlanId(plan_id),
            assigned_at,
            assigned_by,
            replaced_at,
        ))
    }
}

fn parse_dt(s: &str) -> Result<DateTime<Utc>, HostingPlansError> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| HostingPlansError::Persistence(format!("bad timestamp `{s}`: {e}")))
}
