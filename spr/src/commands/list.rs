/*
 * Copyright (c) Radical HQ Limited
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use crate::error::Error;
use crate::error::Result;
use futures::future::try_join_all;
use futures::future::TryFutureExt;
use graphql_client::{GraphQLQuery, Response};
use reqwest;
use std::vec::Vec;

#[allow(clippy::upper_case_acronyms)]
type URI = String;
#[derive(GraphQLQuery)]
#[graphql(
    schema_path = "src/gql/schema.docs.graphql",
    query_path = "src/gql/lookup_review.graphql",
    response_derives = "Debug"
)]
pub struct LookupReview;

pub async fn list(
    graphql_client: reqwest::Client,
    git: &crate::git::Git,
    config: &crate::config::Config,
) -> Result<()> {
    let prepared_commits = git.get_prepared_commits(config)?;

    let responses: Vec<_> = prepared_commits
        .iter()
        .filter_map(|commit| commit.pull_request_number)
        .map(|pr_number| {
            let variables = lookup_review::Variables {
                url: format!(
                    "https://github.com/{}/{}/pull/{}",
                    config.owner, config.repo, pr_number
                ),
            };
            let request_body = LookupReview::build_query(variables);
            graphql_client
                .post("https://api.github.com/graphql")
                .json(&request_body)
                .send()
                .and_then(|res| res.json())
        })
        .collect();

    let response_bodies = try_join_all(responses).await?;

    print_pr_info(response_bodies).ok_or_else(|| Error::new("unexpected error"))
}

fn print_pr_info(
    response_bodies: Vec<Response<lookup_review::ResponseData>>,
) -> Option<()> {
    let term = console::Term::stdout();
    for response in response_bodies {
        let pr = match response.data?.resource {
            Some(lookup_review::LookupReviewResource::PullRequest(pr)) => pr,
            _ => continue,
        };
        let dummy: String;
        let decision = match pr.review_decision {
            Some(lookup_review::PullRequestReviewDecision::APPROVED) => {
                console::style("Accepted").green()
            }
            Some(
                lookup_review::PullRequestReviewDecision::CHANGES_REQUESTED,
            ) => console::style("Changes Needed").red(),
            None
            | Some(lookup_review::PullRequestReviewDecision::REVIEW_REQUIRED) => {
                console::style("Pending")
            }
            Some(lookup_review::PullRequestReviewDecision::Other(d)) => {
                dummy = d;
                console::style(dummy.as_str())
            }
        };
        term.write_line(&format!(
            "{} {} {}",
            decision,
            console::style(&pr.title).bold(),
            console::style(&pr.url).dim(),
        ))
        .ok()?;
    }
    Some(())
}
