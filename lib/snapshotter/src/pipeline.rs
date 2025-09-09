use crate::Manifest;
use kit as u;

fn make_job_def(env: &str, sandbox: &str) -> String {
    format!(r#"
  tc-deploy-topology:
    docker:
      - image: cimg/base:2025.08
    resource_class: large
    parameters:
      tag:
        type: string
      namespace:
        type: string
        default: "default"
      workdir:
        type: string
        default: "default"
      tc_version:
        type: string
    steps:
      - checkout
      - download-tc-latest
      - run: git fetch origin << parameters.tag >>
      - run: git checkout << parameters.tag >>
      - setup_remote_docker:
          docker_layer_caching: true
      - run:
          name: tc-upgrade-<< parameters.tc_version >>
          command: tc upgrade --version << parameters.tc_version >>
      - run:
          name: tc-create-<< parameters.namespace >>
          working_directory: << parameters.workdir >>
          command: tc create -e {env} --sandbox {sandbox} --recursive --trace --sync"#)
}

fn make_job(name: &str, dir: &str, tag: &str, tc_version: &str) -> String {
    format!(r#"
      - tc-deploy-topology:
          name: {name}
          workdir: {dir}
          tag:  {tag}
          tc_version:  {tc_version}
          context:
            - tc
            - cicd-aws-user-creds"#)
}

pub fn generate_config(records: &Vec<Manifest>, env: &str, sandbox: &str) -> String {
    let job_def = make_job_def(env, sandbox);
    let mut jobs: String = String::from("");
    for record in records {
        let Manifest { namespace, dir, version, tc_version, .. } = record;
        let ver = if version.is_empty() {
            "non-existent"
        } else {
            &version
        };

        let tag = format!("{}-{}", namespace, ver);
        let tver = if tc_version.is_empty() {
            "0.9.12"
        } else {
            &tc_version
        };
        let job = make_job(&namespace,  &dir, &tag, tver);
        jobs.push_str(&job);
    }

    let workflow_name = format!("{}-{}-{}-deploy", env, sandbox, u::simple_date());

    format!(r#"
version: 2.1

commands:
  download-tc-latest:
    steps:
      - run:
          name: "Download tc executable"
          command: |
            curl -L -H "Accept: application/octet-stream"  -H "x-github-api-version: 2022-11-28" https://api.github.com/repos/tc-functors/tc/releases/assets/$TC_RELEASE_ID -o tc && chmod +x tc
            sudo mv tc /usr/local/bin/tc

parameters:
  tc-deploy-snapshot-pipeline:
    type: boolean
    default: false
  api_call:
    type: boolean
    default: false
  tc-deploy-env:
    type: string
    default: dev
  tc-deploy-sandbox:
    type: string
    default: main

jobs:
{job_def}

workflows:
  {workflow_name}:
    jobs:
{jobs}

"#)
}
