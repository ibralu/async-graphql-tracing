pub mod starwars;

use crate::starwars::{QueryRoot, StarWars};
use async_graphql::extensions::{Tracing, TracingConfig};
use async_graphql::http::{playground_source, GraphQLPlaygroundConfig};
use async_graphql::{EmptyMutation, EmptySubscription, Schema};
use async_graphql_warp::{BadRequest, Response};
use http::StatusCode;
use std::convert::Infallible;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::registry;
use warp::{http::Response as HttpResponse, Filter, Rejection};

use opentelemetry::trace::{get_active_span, Tracer};
use opentelemetry::KeyValue;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Registry;

#[tokio::main]
async fn main() {
    let (tracer, _unin) = opentelemetry_jaeger::new_pipeline()
        .with_service_name("trace-demo")
        .install()
        .unwrap();

    let opentelemetry = tracing_opentelemetry::layer().with_tracer(tracer);
    let subscriber = Registry::default()
        .with(tracing_subscriber::EnvFilter::new("DEBUG"))
        .with(opentelemetry);

    tracing::subscriber::set_global_default(subscriber).unwrap();

    let schema = Schema::build(QueryRoot, EmptyMutation, EmptySubscription)
        .extension(Tracing::default())
        .data(StarWars::new())
        .finish();

    println!("Playground: http://localhost:5000");

    let graphql_post = async_graphql_warp::graphql(schema).and_then(
        |(schema, request): (
            Schema<QueryRoot, EmptyMutation, EmptySubscription>,
            async_graphql::Request,
        )| async move {
            let span = tracing::info_span!("request");
            let request = request.data(TracingConfig::default().parent_span(span));
            Ok::<_, Infallible>(Response::from(schema.execute(request).await))
        },
    );

    let graphql_playground = warp::path::end().and(warp::get()).map(|| {
        HttpResponse::builder()
            .header("content-type", "text/html")
            .body(playground_source(GraphQLPlaygroundConfig::new("/")))
    });

    let routes = graphql_playground
        .or(graphql_post)
        .recover(|err: Rejection| async move {
            if let Some(BadRequest(err)) = err.find() {
                return Ok::<_, Infallible>(warp::reply::with_status(
                    err.to_string(),
                    StatusCode::BAD_REQUEST,
                ));
            }

            Ok(warp::reply::with_status(
                "INTERNAL_SERVER_ERROR".to_string(),
                StatusCode::INTERNAL_SERVER_ERROR,
            ))
        });

    warp::serve(routes).run(([0, 0, 0, 0], 5000)).await;
}
