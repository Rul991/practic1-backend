use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use std::collections::{HashMap, HashSet};
use tracing::{info, instrument, warn};

use crate::{dto::*, error::AppError, extractors::CurrentUser, jwt, password, state::AppState};

pub async fn register(
    State(state): State<AppState>,
    Json(body): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<UserDto>), AppError> {
    if body.login.len() < 3 || body.password.len() < 6 {
        return Err(AppError::Validation("login >= 3, password >= 6".into()));
    }
    let ph = password::hash(&body.password)?;
    let row = sqlx::query(
        "INSERT INTO users (login, password_hash, name) VALUES ($1, $2, $3) \
         RETURNING id, login, name, role::text AS role, created_at",
    )
    .bind(&body.login)
    .bind(&ph)
    .bind(&body.name)
    .fetch_one(&state.db)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            AppError::Conflict("login taken".into())
        }
        other => AppError::Db(other),
    })?;

    info!(user_id = row.get::<i64, _>("id"), login = %body.login, "user registered");
    Ok((
        StatusCode::CREATED,
        Json(UserDto {
            id: row.get("id"),
            login: row.get("login"),
            name: row.get("name"),
            role: row.get("role"),
            created_at: row.get("created_at"),
        }),
    ))
}

#[instrument(skip(state, body), fields(login = %body.login))]
pub async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, AppError> {
    let row =
        sqlx::query("SELECT id, password_hash, role::text AS role FROM users WHERE login = $1")
            .bind(&body.login)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| {
                warn!("login failed: user not found");
                AppError::Unauthorized
            })?;

    let ph: String = row.get("password_hash");
    if !password::verify(&body.password, &ph) {
        warn!("login failed: bad password");
        return Err(AppError::Unauthorized);
    }
    let id: i64 = row.get("id");
    let role: String = row.get("role");
    let (token, expires_at) = jwt::encode(
        id,
        &role,
        &state.config.jwt_secret,
        state.config.jwt_ttl_secs,
    )?;
    info!(user_id = id, "login ok");
    Ok(Json(LoginResponse { token, expires_at }))
}

pub async fn me(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<UserDto>, AppError> {
    let row = sqlx::query(
        "SELECT id, login, name, role::text AS role, created_at FROM users WHERE id = $1",
    )
    .bind(user.id)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(UserDto {
        id: row.get("id"),
        login: row.get("login"),
        name: row.get("name"),
        role: row.get("role"),
        created_at: row.get("created_at"),
    }))
}

#[instrument(skip(state, body), fields(user_id = user.id, title = %body.title))]
pub async fn create_survey(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<CreateSurveyRequest>,
) -> Result<(StatusCode, Json<SurveyDto>), AppError> {
    if body.title.trim().is_empty() {
        return Err(AppError::Validation("title required".into()));
    }

    let mut positions: HashSet<i32> = HashSet::new();
    for q in &body.questions {
        if !positions.insert(q.position) {
            return Err(AppError::Validation(format!(
                "duplicate question position {}",
                q.position
            )));
        }
        let is_choice = matches!(
            q.qtype,
            QuestionType::SingleChoice | QuestionType::MultipleChoice
        );
        if is_choice && q.options.is_empty() {
            return Err(AppError::Validation(format!(
                "question '{}' needs options",
                q.text
            )));
        }
        if !is_choice && !q.options.is_empty() {
            return Err(AppError::Validation(format!(
                "question '{}' cannot have options",
                q.text
            )));
        }
        let mut opt_pos: HashSet<i32> = HashSet::new();
        for o in &q.options {
            if !opt_pos.insert(o.position) {
                return Err(AppError::Validation(format!(
                    "duplicate option position {} in question '{}'",
                    o.position, q.text
                )));
            }
        }
    }

    let link = nanoid::nanoid!(12);
    let mut tx = state.db.begin().await?;

    let survey_id: i64 = sqlx::query_scalar(
        "INSERT INTO surveys (owner_id, title, description, is_anonymous, only_one_response, link) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(user.id)
    .bind(&body.title)
    .bind(&body.description)
    .bind(body.is_anonymous)
    .bind(body.only_one_response)
    .bind(&link)
    .fetch_one(&mut *tx)
    .await?;

    let created_at: DateTime<Utc> =
        sqlx::query_scalar("SELECT created_at FROM surveys WHERE id = $1")
            .bind(survey_id)
            .fetch_one(&mut *tx)
            .await?;

    let mut questions = Vec::with_capacity(body.questions.len());
    for q in &body.questions {
        let qid: i64 = sqlx::query_scalar(
            "INSERT INTO survey_questions (survey_id, text, type, is_required, position) \
             VALUES ($1, $2, $3::question_type, $4, $5) RETURNING id",
        )
        .bind(survey_id)
        .bind(&q.text)
        .bind(qtype_str(q.qtype))
        .bind(q.is_required)
        .bind(q.position)
        .fetch_one(&mut *tx)
        .await?;

        let mut opts = Vec::with_capacity(q.options.len());
        for o in &q.options {
            let oid: i64 = sqlx::query_scalar(
                "INSERT INTO question_options (question_id, text, position) \
                 VALUES ($1, $2, $3) RETURNING id",
            )
            .bind(qid)
            .bind(&o.text)
            .bind(o.position)
            .fetch_one(&mut *tx)
            .await?;
            opts.push(OptionDto {
                id: oid,
                text: o.text.clone(),
                position: o.position,
            });
        }
        questions.push(QuestionDto {
            id: qid,
            text: q.text.clone(),
            r#type: q.qtype,
            is_required: q.is_required,
            position: q.position,
            options: opts,
        });
    }
    tx.commit().await?;

    info!(survey_id, link = %link, questions = questions.len(), "survey created");
    Ok((
        StatusCode::CREATED,
        Json(SurveyDto {
            id: survey_id,
            link,
            title: body.title,
            description: body.description,
            is_anonymous: body.is_anonymous,
            only_one_response: body.only_one_response,
            created_at,
            questions,
        }),
    ))
}

pub async fn list_own_surveys(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(page): Query<PageParams>,
) -> Result<Json<SurveyListResponse>, AppError> {
    let limit = page.limit_or(50);
    let offset = page.offset_or0();

    let rows = sqlx::query(
        "SELECT s.id, s.title, s.link, s.created_at, \
                (SELECT COUNT(*) FROM responses r WHERE r.survey_id = s.id) AS response_count \
         FROM surveys s \
         WHERE s.owner_id = $1 \
         ORDER BY s.created_at DESC \
         LIMIT $2 OFFSET $3",
    )
    .bind(user.id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(SurveyListResponse {
        items: rows
            .into_iter()
            .map(|r| SurveySummaryDto {
                id: r.get("id"),
                title: r.get("title"),
                link: r.get("link"),
                created_at: r.get("created_at"),
                response_count: r.get::<i64, _>("response_count"),
            })
            .collect(),
    }))
}

pub async fn get_own_survey(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
) -> Result<Json<SurveyDto>, AppError> {
    ensure_owner(&state.db, id, user.id).await?;
    Ok(Json(load_survey(&state.db, id).await?))
}

pub async fn update_survey(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
    Json(body): Json<UpdateSurveyRequest>,
) -> Result<Json<SurveyDto>, AppError> {
    ensure_owner(&state.db, id, user.id).await?;

    if matches!(&body.title, Some(t) if t.trim().is_empty()) {
        return Err(AppError::Validation("title cannot be empty".into()));
    }

    sqlx::query(
        "UPDATE surveys SET \
            title = COALESCE($2, title), \
            description = COALESCE($3, description), \
            is_anonymous = COALESCE($4, is_anonymous), \
            only_one_response = COALESCE($5, only_one_response), \
            updated_at = now() \
         WHERE id = $1",
    )
    .bind(id)
    .bind(&body.title)
    .bind(&body.description)
    .bind(body.is_anonymous)
    .bind(body.only_one_response)
    .execute(&state.db)
    .await?;

    info!(survey_id = id, "survey updated");
    Ok(Json(load_survey(&state.db, id).await?))
}

pub async fn delete_survey(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    let affected = sqlx::query("DELETE FROM surveys WHERE id = $1 AND owner_id = $2")
        .bind(id)
        .bind(user.id)
        .execute(&state.db)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(AppError::NotFound);
    }
    info!(survey_id = id, "survey deleted");
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_public_survey(
    State(state): State<AppState>,
    Path(link): Path<String>,
) -> Result<Json<SurveyDto>, AppError> {
    let id: Option<i64> = sqlx::query_scalar("SELECT id FROM surveys WHERE link = $1")
        .bind(&link)
        .fetch_optional(&state.db)
        .await?;
    let id = id.ok_or(AppError::NotFound)?;
    Ok(Json(load_survey(&state.db, id).await?))
}

#[instrument(skip(state, body), fields(user_id = user.id, link = %link))]
pub async fn submit_response(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(link): Path<String>,
    Json(body): Json<SubmitResponseRequest>,
) -> Result<(StatusCode, Json<SubmitResponseReply>), AppError> {
    let mut tx = state.db.begin().await?;

    let row = sqlx::query("SELECT id, only_one_response FROM surveys WHERE link = $1 FOR UPDATE")
        .bind(&link)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound)?;
    let survey_id: i64 = row.get("id");
    let one_response: bool = row.get("only_one_response");

    if one_response {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM responses WHERE survey_id = $1 AND user_id = $2)",
        )
        .bind(survey_id)
        .bind(user.id)
        .fetch_one(&mut *tx)
        .await?;
        if exists {
            return Err(AppError::Conflict("already responded".into()));
        }
    }

    let qrows = sqlx::query(
        "SELECT id, type::text AS type, is_required FROM survey_questions WHERE survey_id = $1",
    )
    .bind(survey_id)
    .fetch_all(&mut *tx)
    .await?;
    let mut meta: HashMap<i64, (String, bool)> = HashMap::new();
    for q in &qrows {
        meta.insert(q.get("id"), (q.get("type"), q.get("is_required")));
    }

    let mut per_question_count: HashMap<i64, usize> = HashMap::new();
    let mut seen_options: HashSet<(i64, i64)> = HashSet::new();

    for a in &body.answers {
        let (qtype, _) = meta
            .get(&a.question_id)
            .ok_or_else(|| AppError::Validation(format!("unknown question {}", a.question_id)))?;

        let set = (a.option_id.is_some() as i32)
            + (a.text_value.is_some() as i32)
            + (a.numeric_value.is_some() as i32)
            + (a.bool_value.is_some() as i32);
        if set != 1 {
            return Err(AppError::Validation(format!(
                "question {}: exactly one value required",
                a.question_id
            )));
        }

        let ok = match qtype.as_str() {
            "single_choice" | "multiple_choice" => a.option_id.is_some(),
            "text" => a.text_value.is_some(),
            "number" => a.numeric_value.is_some(),
            "boolean" => a.bool_value.is_some(),
            _ => false,
        };
        if !ok {
            return Err(AppError::Validation(format!(
                "answer type mismatch for question {}",
                a.question_id
            )));
        }

        if let Some(s) = &a.text_value {
            if s.chars().count() > 4096 {
                return Err(AppError::Validation(format!(
                    "text too long for question {} (max 4096)",
                    a.question_id
                )));
            }
        }

        if let Some(oid) = a.option_id {
            if !seen_options.insert((a.question_id, oid)) {
                return Err(AppError::Validation(format!(
                    "duplicate option {} for question {}",
                    oid, a.question_id
                )));
            }
        }

        *per_question_count.entry(a.question_id).or_default() += 1;
    }

    for (qid, count) in &per_question_count {
        let (qtype, _) = &meta[qid];
        if qtype == "single_choice" && *count > 1 {
            return Err(AppError::Validation(format!(
                "single_choice question {} got {} answers",
                qid, count
            )));
        }
    }

    let answered: HashSet<i64> = body.answers.iter().map(|a| a.question_id).collect();
    for (qid, (_, is_required)) in &meta {
        if *is_required && !answered.contains(qid) {
            return Err(AppError::Validation(format!(
                "required question {} missing",
                qid
            )));
        }
    }

    let response_id: i64 = sqlx::query_scalar(
        "INSERT INTO responses (survey_id, user_id) VALUES ($1, $2) RETURNING id",
    )
    .bind(survey_id)
    .bind(user.id)
    .fetch_one(&mut *tx)
    .await?;

    for a in &body.answers {
        sqlx::query(
            "INSERT INTO answers (response_id, question_id, option_id, text_value, numeric_value, bool_value) \
             VALUES ($1, $2, $3, $4, $5, $6)")
            .bind(response_id).bind(a.question_id).bind(a.option_id)
            .bind(&a.text_value).bind(a.numeric_value).bind(a.bool_value)
            .execute(&mut *tx).await?;
    }

    let created_at: DateTime<Utc> =
        sqlx::query_scalar("SELECT created_at FROM responses WHERE id = $1")
            .bind(response_id)
            .fetch_one(&mut *tx)
            .await?;

    tx.commit().await?;
    info!(
        response_id,
        survey_id,
        answers = body.answers.len(),
        "response submitted"
    );
    Ok((
        StatusCode::CREATED,
        Json(SubmitResponseReply {
            response_id,
            created_at,
        }),
    ))
}

pub async fn get_results(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, AppError> {
    ensure_owner(&state.db, id, user.id).await?;

    let total_responses: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM responses WHERE survey_id = $1")
            .bind(id)
            .fetch_one(&state.db)
            .await?;

    let qrows = sqlx::query(
        "SELECT id, text, type::text AS type FROM survey_questions \
         WHERE survey_id = $1 ORDER BY position",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;

    let mut questions = Vec::with_capacity(qrows.len());
    for q in qrows {
        let qid: i64 = q.get("id");
        let qtype: String = q.get("type");
        let text: String = q.get("text");

        let data =
            match qtype.as_str() {
                "single_choice" | "multiple_choice" => {
                    let opts = sqlx::query(
                        "SELECT o.id, o.text, COUNT(a.id) AS cnt \
                     FROM question_options o \
                     LEFT JOIN answers a ON a.option_id = o.id \
                     WHERE o.question_id = $1 \
                     GROUP BY o.id, o.text, o.position \
                     ORDER BY o.position",
                    )
                    .bind(qid)
                    .fetch_all(&state.db)
                    .await?;
                    serde_json::json!(
                        opts.into_iter()
                            .map(|r| serde_json::json!({
                                "option_id": r.get::<i64, _>("id"),
                                "text":      r.get::<String, _>("text"),
                                "count":     r.get::<i64, _>("cnt"),
                            }))
                            .collect::<Vec<_>>()
                    )
                }
                "boolean" => {
                    let t: i64 = sqlx::query_scalar(
                        "SELECT COUNT(*) FROM answers WHERE question_id = $1 AND bool_value = TRUE",
                    )
                    .bind(qid)
                    .fetch_one(&state.db)
                    .await?;
                    let f: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM answers WHERE question_id = $1 AND bool_value = FALSE")
                    .bind(qid).fetch_one(&state.db).await?;
                    serde_json::json!({ "true": t, "false": f })
                }
                "number" => {
                    let row = sqlx::query(
                        "SELECT AVG(numeric_value)::float8 AS avg, \
                            MIN(numeric_value)::float8 AS min, \
                            MAX(numeric_value)::float8 AS max, \
                            COUNT(*) AS cnt \
                     FROM answers WHERE question_id = $1",
                    )
                    .bind(qid)
                    .fetch_one(&state.db)
                    .await?;
                    serde_json::json!({
                        "count": row.get::<i64, _>("cnt"),
                        "avg":   row.get::<Option<f64>, _>("avg"),
                        "min":   row.get::<Option<f64>, _>("min"),
                        "max":   row.get::<Option<f64>, _>("max"),
                    })
                }
                "text" => {
                    let rows = sqlx::query(
                        "SELECT text_value FROM answers WHERE question_id = $1 \
                     ORDER BY id LIMIT 1000",
                    )
                    .bind(qid)
                    .fetch_all(&state.db)
                    .await?;
                    serde_json::json!(
                        rows.into_iter()
                            .filter_map(|r| r.get::<Option<String>, _>("text_value"))
                            .collect::<Vec<_>>()
                    )
                }
                _ => serde_json::json!(null),
            };

        questions.push(serde_json::json!({
            "id": qid, "text": text, "type": qtype, "data": data,
        }));
    }

    Ok(Json(serde_json::json!({
        "survey_id": id,
        "total_responses": total_responses,
        "questions": questions,
    })))
}

pub async fn list_responses(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<i64>,
    Query(page): Query<PageParams>,
) -> Result<Json<ResponseListResponse>, AppError> {
    ensure_owner(&state.db, id, user.id).await?;

    let is_anonymous: bool = sqlx::query_scalar("SELECT is_anonymous FROM surveys WHERE id = $1")
        .bind(id)
        .fetch_one(&state.db)
        .await?;
    if is_anonymous {
        return Err(AppError::Forbidden);
    }

    let limit = page.limit_or(100);
    let offset = page.offset_or0();

    let rows = sqlx::query(
        "SELECT r.id, r.created_at, u.id AS uid, u.login, u.name \
         FROM responses r JOIN users u ON u.id = r.user_id \
         WHERE r.survey_id = $1 \
         ORDER BY r.created_at DESC \
         LIMIT $2 OFFSET $3",
    )
    .bind(id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let response_ids: Vec<i64> = rows.iter().map(|r| r.get::<i64, _>("id")).collect();

    let mut answers_by_response: HashMap<i64, Vec<AnswerDetailDto>> = HashMap::new();
    if !response_ids.is_empty() {
        let answer_rows = sqlx::query(
            "SELECT a.response_id, \
                    a.question_id, \
                    q.text AS question_text, \
                    a.option_id, \
                    o.text AS option_text, \
                    a.text_value, a.numeric_value, a.bool_value \
             FROM answers a \
             JOIN survey_questions q ON q.id = a.question_id \
             LEFT JOIN question_options o ON o.id = a.option_id \
             WHERE a.response_id = ANY($1) \
             ORDER BY a.response_id, q.position, a.id",
        )
        .bind(&response_ids)
        .fetch_all(&state.db)
        .await?;

        for r in answer_rows {
            let rid: i64 = r.get("response_id");
            answers_by_response
                .entry(rid)
                .or_default()
                .push(AnswerDetailDto {
                    question_id: r.get("question_id"),
                    question_text: r.get("question_text"),
                    option_id: r.get("option_id"),
                    option_text: r.get("option_text"),
                    text_value: r.get("text_value"),
                    numeric_value: r.get("numeric_value"),
                    bool_value: r.get("bool_value"),
                });
        }
    }

    Ok(Json(ResponseListResponse {
        items: rows
            .into_iter()
            .map(|r| {
                let rid: i64 = r.get("id");
                ResponseListItem {
                    response_id: rid,
                    created_at: r.get("created_at"),
                    user: Some(UserBriefDto {
                        id: r.get("uid"),
                        login: r.get("login"),
                        name: r.get("name"),
                    }),
                    answers: Some(answers_by_response.remove(&rid).unwrap_or_default()),
                }
            })
            .collect(),
    }))
}

async fn ensure_owner(pool: &PgPool, survey_id: i64, user_id: i64) -> Result<(), AppError> {
    let owner: Option<i64> = sqlx::query_scalar("SELECT owner_id FROM surveys WHERE id = $1")
        .bind(survey_id)
        .fetch_optional(pool)
        .await?;
    match owner {
        None => Err(AppError::NotFound),
        Some(o) if o != user_id => Err(AppError::Forbidden),
        _ => Ok(()),
    }
}

async fn load_survey(pool: &PgPool, id: i64) -> Result<SurveyDto, AppError> {
    let row = sqlx::query(
        "SELECT id, title, description, is_anonymous, only_one_response, created_at, link \
         FROM surveys WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)?;

    let qrows = sqlx::query(
        "SELECT id, text, type::text AS type, is_required, position \
         FROM survey_questions WHERE survey_id = $1 ORDER BY position",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;

    let mut questions = Vec::with_capacity(qrows.len());
    for q in qrows {
        let qid: i64 = q.get("id");
        let orows = sqlx::query(
            "SELECT id, text, position FROM question_options \
             WHERE question_id = $1 ORDER BY position",
        )
        .bind(qid)
        .fetch_all(pool)
        .await?;
        questions.push(QuestionDto {
            id: qid,
            text: q.get("text"),
            r#type: parse_qtype(&q.get::<String, _>("type")),
            is_required: q.get("is_required"),
            position: q.get("position"),
            options: orows
                .into_iter()
                .map(|o| OptionDto {
                    id: o.get("id"),
                    text: o.get("text"),
                    position: o.get("position"),
                })
                .collect(),
        });
    }

    Ok(SurveyDto {
        id: row.get("id"),
        link: row.get("link"),
        title: row.get("title"),
        description: row.get("description"),
        is_anonymous: row.get("is_anonymous"),
        only_one_response: row.get("only_one_response"),
        created_at: row.get("created_at"),
        questions,
    })
}

fn qtype_str(t: QuestionType) -> &'static str {
    match t {
        QuestionType::SingleChoice => "single_choice",
        QuestionType::MultipleChoice => "multiple_choice",
        QuestionType::Text => "text",
        QuestionType::Number => "number",
        QuestionType::Boolean => "boolean",
    }
}

fn parse_qtype(s: &str) -> QuestionType {
    match s {
        "single_choice" => QuestionType::SingleChoice,
        "multiple_choice" => QuestionType::MultipleChoice,
        "text" => QuestionType::Text,
        "number" => QuestionType::Number,
        _ => QuestionType::Boolean,
    }
}
