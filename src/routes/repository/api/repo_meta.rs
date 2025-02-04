use crate::privileges::privilege;
use crate::repository::Repository;
use crate::routes::repository::GitRequest;
use crate::user::WebUser;
use crate::{die, err};

use actix_web::{HttpResponse, Responder, web};
use anyhow::Result;
use gitarena_macros::route;
use sqlx::PgPool;

#[route("/api/repo/{username}/{repository}", method = "GET", err = "json")]
pub(crate) async fn meta(uri: web::Path<GitRequest>, web_user: WebUser, db_pool: web::Data<PgPool>) -> Result<impl Responder> {
    let mut transaction = db_pool.begin().await?;

    let (user_id,): (i32,) = sqlx::query_as("select id from users where lower(username) = lower($1) limit 1")
        .bind(&uri.username)
        .fetch_optional(&mut transaction)
        .await?
        .ok_or_else(|| err!(NOT_FOUND, "Not found"))?;

    let repo: Repository = sqlx::query_as::<_, Repository>("select * from repositories where owner = $1 and lower(name) = lower($2) limit 1")
        .bind(&user_id)
        .bind(&uri.repository)
        .fetch_optional(&mut transaction)
        .await?
        .ok_or_else(|| err!(NOT_FOUND, "Not found"))?;

    if !privilege::check_access(&repo, web_user.as_ref(), &mut transaction).await? {
        die!(NOT_FOUND, "Not found");
    }

    transaction.commit().await?;

    Ok(HttpResponse::Ok().json(repo))
}
