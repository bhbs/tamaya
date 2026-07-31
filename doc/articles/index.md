---
lang: en-US
title: Understanding Tamaya
description: A three-part series separating the single-VPS model, Linux as an application platform, and the deployment model Tamaya adds.
---

# Understanding Tamaya

This series explains the ideas behind Tamaya in three distinct layers: the infrastructure model, the Linux application platform, and the deployment tooling built on top.

## 1. Choosing a Single VPS

[Why Indie Developers Should Consider a Single VPS](./why-single-vps.md)

A comparison with architectures assembled from multiple managed cloud services, covering cost, performance, security, operability for AI agents, and the implications of a single point of failure.

## 2. Using Linux as an Application Platform

[How Far Can Linux Go as an Application Platform Without Containers?](./linux-application-platform.md)

An explanation of what the Linux kernel, systemd, cgroups, namespaces, Caddy, and SSH each provide—and how far they can isolate applications on one host.

## 3. The Deployment Model Tamaya Adds

[How Tamaya Turns a Linux Server into a Deployment Platform](./how-tamaya-uses-linux.md)

How Tamaya combines Linux primitives into single-binary deployment, health-checked release switching, and operations through a small CLI and text configuration.
