pub mod appsync;
pub mod bootstrap;
pub mod cache;
pub mod cloudwatch;
mod default;
pub mod dynamo;
pub mod ec2;
pub mod efs;
pub mod eventbridge;
pub mod gateway;
pub mod gatewayv2;
pub mod iam;
pub mod lambda;
pub mod layer;
pub mod s3;
pub mod scheduler;
pub mod sfn;
pub mod sqs;
pub mod ssm;
pub mod sts;

use aws_config::SdkConfig;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;

use kit::*;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Env {
    pub name: String,
}

// config

pub async fn init(profile: &Option<String>) {
    let name = maybe_string(profile.clone(), "dev");
    let env = Env::new(&name);
    let client = sts::make_client(&env).await;
    let account = sts::get_account_id(&client).await;
    cache::write(&name, &account).await;
}

impl Env {
    pub fn new(name: &str) -> Env {
        Env {
            name: String::from(name),
        }
    }

    pub fn maybe(some_name: Option<String>) -> Env {
        let name = match some_name {
            Some(name) => name,
            None => String::from("dev"),
        };
        Env { name: name }
    }

    pub async fn load(&self) -> SdkConfig {
        aws_config::from_env().profile_name(&self.name).load().await
    }

    pub fn account(&self) -> String {
        cache::read(&self.name)
    }

    pub fn region(&self) -> String {
        match std::env::var("AWS_REGION") {
            Ok(e) => e,
            Err(_) => String::from("us-west-2"),
        }
    }

    pub fn sfn_uri(&self) -> String {
        format!(
            "arn:aws:apigateway:{}:states:action/StartSyncExecution",
            &self.region()
        )
    }

    pub fn sfn_arn(&self, name: &str) -> String {
        format!(
            "arn:aws:states:{}:{}:stateMachine:{}",
            &self.region(),
            self.account(),
            name
        )
    }

    pub fn lambda_arn(&self, name: &str) -> String {
        format!(
            "arn:aws:lambda:{}:{}:function:{}",
            &self.region(),
            &self.account(),
            name
        )
    }

    pub fn layer_arn(&self, name: &str) -> String {
        format!(
            "arn:aws:lambda:{}:{}:layer:{}",
            &self.region(),
            &self.account(),
            name
        )
    }

    pub fn role_arn(&self, name: &str) -> String {
        format!("arn:aws:iam::{}:role/{}", &self.account(), name)
    }

    pub fn policy_arn(&self, name: &str) -> String {
        format!("arn:aws:iam::{}:policy/{}", &self.account(), name)
    }

    pub fn event_bus_arn(&self, bus_name: &str) -> String {
        format!(
            "arn:aws:events:{}:{}:event-bus/{}",
            &self.region(),
            &self.account(),
            bus_name
        )
    }

    pub fn api_endpoint(&self, api_id: &str, stage: &str) -> String {
        format!(
            "https://{}.execute-api.{}.amazonaws.com/{}",
            api_id,
            self.region(),
            stage
        )
    }

    pub fn sfn_url(&self, name: &str, id: &str) -> String {
        format!("https://{}.console.aws.amazon.com/states/home?region={}#/v2/executions/details/arn:aws:states:{}:{}:execution:{}:{}",
                &self.region(), &self.region(),
                &self.region(), &self.account(), name, id)
    }

    pub fn sfn_url_express(&self, arn: &str) -> String {
        format!("https://{}.console.aws.amazon.com/states/home?region={}#/express-executions/details/{}?startDate={}", &self.region(), &self.region(), arn, kit::current_millis() - 200000)
    }

    //config
    pub fn base_role(&self, name: &str) -> String {
        format!("tc-base-{}-role", name)
    }

    pub fn base_policy(&self, name: &str) -> String {
        format!("tc-base-{}-policy", name)
    }

    pub fn api_integration_arn(&self, lambda_arn: &str) -> String {
        format!(
            "arn:aws:apigateway:us-west-2:lambda:path/2015-03-31/functions/{}/invocations",
            lambda_arn
        )
    }

    pub fn api_arn(&self, api_id: &str) -> String {
        format!(
            "arn:aws:execute-api:{}:{}:{}/*/*/*",
            &self.region(),
            &self.account(),
            api_id
        )
    }

    pub fn graphql_arn(&self, id: &str) -> String {
        format!(
            "arn:aws:appsync:{}:{}:endpoints/graphql-api/{}",
            &self.region(),
            &self.account(),
            id
        )
    }

    pub fn sqs_arn(&self, name: &str) -> String {
        format!(
            "arn:aws:sqs:{}:{}:{}",
            &self.region(),
            &self.account(),
            name
        )
    }

    pub fn sqs_url(&self, name: &str) -> String {
        format!(
            "https://sqs.{}.amazonaws.com/{}/{}",
            &self.region(),
            &self.account(),
            name
        )
    }

    // resolvers

    pub async fn resolve_layers(&self, layers: Vec<String>) -> Vec<String> {
        let client = layer::make_client(&self).await;
        let mut v: Vec<String> = vec![];
        for layer in layers {
            let arn = layer::find_version(client.clone(), &layer).await.unwrap();
            v.push(arn);
        }
        v
    }

    pub async fn resolve_layer(&self, layer_name: &str) -> String {
        let client = layer::make_client(&self).await;
        layer::find_version(client, layer_name).await.unwrap()
    }

    pub async fn resolve_vars(
        &self,
        environment: HashMap<String, String>,
    ) -> HashMap<String, String> {
        let client = ssm::make_client(&self).await;

        let mut h: HashMap<String, String> = HashMap::new();
        for (k, v) in environment.iter() {
            if v.starts_with("ssm:/") {
                let key = kit::split_last(v, ":");
                let val = ssm::get(client.clone(), &key).await.unwrap();
                h.insert(s!(k), val);
            } else {
                h.insert(s!(k), s!(v));
            }
        }
        h
    }

    pub async fn subnets(&self, tag: &str) -> Vec<String> {
        ec2::get_subnets(&self, tag).await.unwrap()
    }

    pub async fn security_groups(&self, tag: &str) -> Vec<String> {
        ec2::get_security_groups(&self, tag).await.unwrap()
    }

    pub async fn access_point_arn(&self, name: &str) -> Option<String> {
        efs::get_ap_arn(&self, name).await.unwrap()
    }

    // policies
    pub fn base_trust_policy(&self) -> String {
        default::trust_policy()
    }

    pub fn base_lambda_policy(&self) -> String {
        default::lambda_policy()
    }

    pub fn base_sfn_policy(&self) -> String {
        default::sfn_policy()
    }

    pub fn base_api_policy(&self) -> String {
        default::api_policy()
    }

    pub fn base_event_policy(&self) -> String {
        default::event_policy(&self.region(), &self.account())
    }

    pub fn base_appsync_policy(&self) -> String {
        default::appsync_policy(&self.region(), &self.account())
    }
}
