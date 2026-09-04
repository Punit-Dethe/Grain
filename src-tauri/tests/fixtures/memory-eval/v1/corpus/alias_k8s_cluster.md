---
grain_id: 66666666-6666-4666-8666-666666666601
title: K8s Production Cluster Operations
timestamp: 1786406400000
tldr: Runbook for the Kubernetes (k8s / prod cluster / EKS) deployment cluster.
question: What are the operational procedures for the production k8s cluster?
reminder: ""
pinned: false
entities:
  - k8s
  - kubernetes
  - prod_cluster
  - eks
  - devops
todos: []
source: test_fixture
---

# Kubernetes Production Cluster Runbook

This document covers operational tasks for our production Kubernetes cluster (commonly referred to as 'k8s', 'EKS', or 'prod cluster').

## Cluster Details
- Provider: AWS EKS
- Node groups: 3 × m6i.xlarge instances across us-west-2a/b/c.
- Deployments: Managed through ArgoCD.
