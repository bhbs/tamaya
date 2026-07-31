---
lang: en-US
title: Why Indie Developers Should Consider a Single VPS
description: An examination of the cost, performance, security, and AI-assisted operational advantages—and limits—that a single VPS offers independent developers compared with a cloud architecture composed of multiple managed services.
---

# Why Indie Developers Should Consider a Single VPS

When you start designing a production environment for an independent project, the architecture can grow surprisingly quickly: a load balancer, an application runtime, a managed database, object storage, secret management, monitoring, and IAM. Each component is reasonable on its own, but once combined, they leave you operating a second product alongside your application: the infrastructure itself.

A single VPS, meanwhile, is often seen as an architecture that trades safety and speed for a lower price. But for a small, continuously running service maintained by one person or a small team, a single Linux VPS is not merely a compromise. It can offer a better balance of performance, security, and operational overhead than a multi-service architecture.

The reason is not that everything should simply be crammed onto one machine. It is that you can avoid creating unnecessary network boundaries and operational responsibilities in the first place, maintaining only the boundaries required by your threat model and availability requirements.

This article examines the single VPS as a **deployment and operating model**. How Linux provides process supervision and isolation, and how deployment tools automate those mechanisms, belong to separate layers; they are covered in the two follow-up articles linked at the end.

## The Real Comparison Is Not “VPS vs. Cloud”

A VPS is itself usually a virtual machine supplied by a cloud provider. The comparison here is not between physical servers and the cloud, but between two operating models:

- A single-VPS architecture that colocates the reverse proxy, applications, and persistent data on one Linux host
- A cloud architecture that distributes the runtime, database, storage, and networking functions across multiple managed services

Managed services offer clear benefits: operating-system maintenance can be transferred to the provider, horizontal scaling and redundancy are easier to build in, and fine-grained access control and audit facilities are readily available. This article does not dispute those benefits. It asks whether a small service needs those capabilities—and their accompanying complexity—from day one.

A single VPS is often a good fit for services with the following characteristics:

- They are developed and operated by one person or a small team.
- Traffic is low to moderate, but the service should remain continuously running rather than scale to zero.
- CPU, memory, disk, and write workloads fit on one machine.
- A single region is sufficient, and recovery from backups is acceptable after a failure.
- Untrusted third-party code is not colocated on the host.
- A recovery time ranging from minutes to hours is acceptable.

Conversely, if uninterrupted operation, horizontal scaling, multiple regions, or stringent regulatory requirements are present from the outset, the premise of the comparison changes. The important thing is to select an architecture that fits the current workload and failure tolerance—not the largest scale the project might someday reach.

| Dimension | Single VPS | Multi-service cloud architecture |
| --- | --- | --- |
| Request path | The reverse proxy, application, and data can reside on the same host | Requests often traverse several network services |
| Resources | Multiple applications share one pool of CPU, memory, and disk | Provisioned services tend to carry capacity or a minimum allocation for each service |
| Security management | SSH, the OS, process privileges, and exposed ports are managed centrally | IAM, networking, and service-specific policies must be managed across services |
| Scaling | Primarily vertical | Horizontal and geographic scaling are easier to design |
| Failure domain | A host failure stops every colocated application | Failure domains can be separated when the architecture is designed accordingly |

## Decoupling Cost from the Number of Applications

In a multi-service cloud architecture, the application runtime, load balancer, database, and other components often each impose a minimum allocation or fixed cost. Provisioning the complete minimum architecture for a small, continuously running service means paying for the entire stack even during mostly idle periods. If every new application adds another set of the same components, the number of billable items grows faster than the user base. Workloads suited to usage-based pricing or scale-to-zero are exceptions, but if continuous responsiveness and several provisioned services are required, the cost of each layer remains.

With a single VPS, one machine's CPU, memory, disk, and bandwidth form a shared pool for multiple applications. Small indie services do not necessarily peak at the same time. As long as their combined load fits on one machine, the same fixed cost can support more continuously running processes than an architecture that reserves a minimum dedicated instance for every application. Costs then reflect aggregate resource consumption rather than the number of provisioned services.

The operator's time is also a cost. Every additional service introduces another set of permissions, credentials, network connections, alerts, and release notes to understand. For a solo developer, attention can run out before the monthly budget does.

Of course, maintaining the VPS operating system, backups, and disaster recovery becomes your responsibility. If a managed service genuinely takes that work off your hands, it can be worth paying for. The comparison should cover not only the invoice, but total cost: the maintenance time you assume and the responsibilities you can transfer to a provider.

## The Performance Advantage Comes from Proximity

The performance strength of a single VPS does not come from a particular executable format or packaging method. It comes from keeping data and computation close together and reducing the amount of work required for each request.

A typical request path is short:

```text
Internet -> reverse proxy -> localhost application -> local database / files
```

Placing the application and database on the same host avoids network round trips to another machine. With SQLite on local disk, no separate database server process is required; the application accesses the data through a library. The [official SQLite documentation](https://www.sqlite.org/whentouse.html) likewise explains that SQLite is appropriate for many low- to medium-traffic websites and that colocating it with the application avoids network round trips.

SQLite is not universal, however. It permits only one writer at a time, so a client/server database is a better fit for write-intensive services or architectures in which multiple hosts access the same data. Running PostgreSQL or another database on the same VPS is also an option, but then its upgrades, monitoring, and backups become your responsibility as well.

Keeping applications running continuously also avoids cold starts after idle periods. This is a particularly good fit for small APIs, webhooks, admin interfaces, and bots that receive infrequent requests but should respond immediately.

Simply consolidating workloads on one machine does not make them fast. Applications competing for CPU, slow disks, insufficient memory, or unbounded logs can affect every colocated process. Without measurement and resource limits, contention can outweigh the advantages of proximity.

Nor does this argument assume that native execution is always faster than containers. What a single VPS primarily eliminates is remote service dependencies, network round trips between services, and surplus capacity allocated per service. The performance advantage comes from a shorter architecture, not benchmark magic.

## Separate Attack Surface from Blast Radius

Security is not determined solely by the number of products or servers in use. [NIST defines](https://csrc.nist.gov/glossary/term/attack_surface) an attack surface as the set of points on a system boundary through which an attacker can enter, affect the system, or extract data. As public endpoints, credentials, IAM roles, inter-service connections, and configuration policies accumulate, so does the ongoing work required to verify that each remains correct.

A properly configured single VPS makes it relatively easy to keep external boundaries small:

```text
Internet ------------> reverse proxy :80/:443 ----> localhost applications
Operator workstation -> SSH ----------------------> maintenance and deployment
```

Rather than exposing database and application ports directly to the Internet, HTTP(S) ingress can be consolidated at the reverse proxy and administrative access can be consolidated through SSH. With fewer things to defend, one person can more readily review the firewall, logs, keys, and update status. This **comprehensibility** is a practical security advantage for a small team.

A small attack surface is not, however, the same thing as a small blast radius:

- If root privileges or the host kernel are compromised, every colocated application and its data may be affected.
- Excessive load or disk exhaustion in one application can disrupt another.
- Per-process privilege separation impedes lateral movement, but it does not provide an isolation boundary equivalent to separate VMs.
- Backups stored only on the same VPS can be lost alongside the host through failure, deletion, or compromise.

Managed services, by contrast, can transfer maintenance of the operating system and underlying infrastructure to the provider while separating permissions and failure domains by service. But user responsibility does not disappear. The [AWS shared responsibility model](https://aws.amazon.com/compliance/shared-responsibility-model/), for example, leaves data, access control, applications, and configuration of the selected services with the customer.

> A single VPS is not secure merely because it is a single VPS. Security comes from simplicity that lets you understand the whole system and consistently defend the boundaries appropriate to your threat model.

A multi-tenant platform that executes untrusted third-party code should consider boundaries stronger than ordinary process isolation on a shared kernel, such as separate hosts, VMs, microVMs, or sandboxed container runtimes. For the threat model of running several small applications that you control, however, limiting exposure and separating applications by privileges and resources is a practical design.

## Linux Can Create Boundaries Within One Host

A single VPS need not mean carelessly running every application under the same user account. Linux provides the basic mechanisms required to operate a server: per-application users and file permissions, process supervision and automatic restarts, CPU and memory controls, restrictions on filesystem and temporary-directory access, and log collection.

Crucially, these are neither inherent VPS features nor inventions of a particular deployment product. Operators and tools combine mechanisms supplied by the Linux kernel, cgroups, namespaces, systemd, and related components. Containers use many of the same kernel mechanisms, but differ from systemd services in that an image bundles userspace and a root filesystem while the runtime and surrounding tooling impose a common model for networking and lifecycle management.

[How Far Can Linux Go as an Application Platform Without Containers?](./linux-application-platform.md) examines in detail how much isolation is possible within one host and where the limits of a shared kernel begin.

## Reducing the Decision Space an AI Must Navigate

Cloud platforms are not inherently difficult for AI agents to operate. They provide structured APIs and CLIs, and with infrastructure as code, tools such as [Terraform plan](https://developer.hashicorp.com/terraform/cli/commands/plan) and [CloudFormation change sets](https://docs.aws.amazon.com/AWSCloudFormation/latest/UserGuide/using-cfn-updating-stacks-changesets.html) can preview changes to managed resources before applying them. A cloud architecture can support tighter organizational controls over AI-driven operations by issuing temporary, least-privilege access in line with [AWS IAM best practices](https://docs.aws.amazon.com/IAM/latest/UserGuide/best-practices.html), and by auditing API activity through [CloudTrail](https://docs.aws.amazon.com/awscloudtrail/latest/userguide/cloudtrail-user-guide.html) with an explicitly configured scope and retention period.

The difference is not whether an AI can issue a command, but the breadth of state it must understand and validate before and after proposing that command. When the runtime, load balancer, network, DNS, database, secrets, IAM, and monitoring are separated, even one change requires reconciling dependencies between resources, the target account and region, the actual deployed state, the infrastructure-as-code state, and any drift between them. IaC represents that resource graph as code; it does not remove the graph.

On a single VPS, the deployment surface can be constrained to one host, a reverse proxy, processes, configuration, and local data. When an AI agent has fewer configuration files to read and commands to run, a human can also review the proposed change and its failure modes more easily.

| Dimension | Single VPS | Multi-service cloud architecture |
| --- | --- | --- |
| Primary operational surface | Host, reverse proxy, processes, configuration, local storage | Runtime, network, load balancer, DNS, database, secrets, IAM, and more |
| State and dependencies | Relatively small state within one machine | Resource graph, account/region, actual resources, IaC state, and drift |
| Permissions | SSH and sudo are simple but can confer broad privileges | Temporary credentials and fine-grained IAM can be designed |
| Pre-change review | Diffs, dry runs, and procedures must be established by the operator | IaC can provide plans, change sets, and similar previews |
| Auditing | OS and tool logs must be configured by the operator | Provider audit events can be retained for a configured scope and period |

This advantage does not mean you should give an AI agent a production SSH key. SSH credentials and sudo access should be treated as powerful privileges capable of modifying the entire host. An AI agent can propose configuration changes and commands, while deployment, secret changes, and data deletion remain subject to human approval. Where possible, separate read privileges from write privileges and avoid permanent credentials.

Likewise, if the operating procedure consists solely of ad hoc commands after an SSH login, one machine is not inherently safe or easy for an AI to manage. Operations that are tractable for both AI and humans require the following:

- Keep small text configuration files as the source of truth.
- Use non-interactive commands whose results are easy to inspect.
- Show a diff or fully resolved configuration before making a change.
- Make the current processes, releases, and logs observable.
- Define recovery procedures and human approval boundaries.

The cloud's strength is fine-grained operational control; the single VPS's strength is having fewer things to control. Which is better suited to AI-assisted operations depends not only on change complexity, but on the required degree of privilege separation and auditing.

## Do Not Confuse a Single Point of Failure with Performance or Security

If the VPS stops, every application on it stops. Whether the cause is hardware failure, a provider outage, a reboot after an operating-system update, or operator error, one machine offers no automatic failover. This is a clear limitation of the single-VPS model.

It is primarily an **availability** problem, however. The presence of one machine alone does not imply poor performance or immediate insecurity. Conversely, even a redundant multi-node architecture is not secure if its exposure or IAM configuration is wrong, and additional dependencies over the network introduce both latency and more failure modes.

At the same time, a single point of failure also affects blast radius and recovery time. A host-wide compromise, disk failure, or account suspension can take every colocated service with it. You must therefore evaluate “one machine has enough processing capacity” separately from “an outage of that machine is acceptable.”

> Distribution is a means of achieving availability and scale, not a free upgrade to performance and security.

## Responsibilities You Assume When Choosing a Single VPS

A simple architecture is not a maintenance-free architecture. Before adopting it, verify that you can assume at least the following responsibilities:

- Update the OS, reverse proxy, and application dependencies, with a patching procedure that accounts for reboots.
- Restrict exposed ports and SSH/sudo privileges, and set per-application execution privileges and resource limits.
- Define procedures for storing, rotating, and revoking secrets.
- Back up data to a separate failure domain and actually test restoration onto a new VPS.
- Alert on external availability, errors, resource consumption, certificate status, and backup failures.
- Document recovery procedures, including host reconstruction, DNS cutover, and the target recovery time.

Persistence is not backup. Provider snapshots are useful, but if they live only in the same account or region, they may not be independent of every relevant failure. Decide which failures you need to survive, then design the backup location, keys, and restoration procedure accordingly.

## When to Move to a Cloud Architecture

Remaining on a single VPS should not become a goal in itself. When any of the following requirements emerge, the value of managed services or a multi-node architecture begins to outweigh their complexity:

- The service cannot stop even during a single machine failure or maintenance window.
- CPU, memory, disk, or bandwidth approaches the limit of one machine.
- Increasing write contention makes a single-host database a bottleneck.
- The service must deliver low latency from multiple geographic regions.
- It executes untrusted third-party code.
- The platform must provide team-level access control, centralized auditing, or regulatory compliance.
- Managed database replication or point-in-time recovery becomes more valuable than its operating cost.
- Manual restoration from backup cannot meet the business's recovery time objective (RTO) or recovery point objective (RPO).

This is not a failure of the single-VPS model. It means that real requirements now justify the next architecture. Rather than building tomorrow's distributed system prematurely, start with a simple architecture, measure it, and separate only those components where bottlenecks or availability requirements actually emerge. The reason for each migration then remains clear.

The Google SRE Workbook similarly argues that simple systems are less likely to fail and easier to understand, maintain, test, and repair, emphasizing [end-to-end simplicity](https://sre.google/workbook/simplicity/) from architecture through operational processes. Simplicity is not immaturity; it is a design choice to meet the required reliability with the smallest sufficient architecture.

## Conclusion

The maturity of infrastructure for an independent project is not measured by its service count. What matters more is whether it meets the required performance, provides boundaries appropriate to the threat model, and can be updated, monitored, and restored by one person.

For an appropriate workload, a single VPS offers the following advantages:

- Applications and data remain close, avoiding unnecessary network round trips.
- Fixed capacity can be shared efficiently among several small, continuously running applications.
- Fewer public surfaces, credentials, and configuration targets make the whole system easier to understand.
- The operational surface and state transitions that AI and humans must understand and verify remain small.
- Weaknesses such as backup recovery and host failure are explicit, making it possible to adopt the next architecture when requirements change.

These are not inherent guarantees of a single VPS. They are properties that a properly configured single-host operating model makes easier to achieve. They should be considered separately from Linux isolation mechanisms and the features of any particular deployment tool. The next two articles examine how to build a safe application platform within one machine and how to connect that platform to reproducible deployments.

- [How Far Can Linux Go as an Application Platform Without Containers?](./linux-application-platform.md)
- [How Tamaya Turns a Linux Server into a Deployment Platform](./how-tamaya-uses-linux.md)

## References and Primary Sources

The terminology, product capabilities, and operational characteristics discussed above were checked against the following official sources (last reviewed July 31, 2026). Individual product capabilities can be verified in these sources; the conclusion about which architecture is appropriate is a design judgment based on the workload and operating conditions described in this article.

### Data Placement and Security

- [SQLite — Appropriate Uses For SQLite](https://www.sqlite.org/whentouse.html)
  Used to verify SQLite's suitability for low- to medium-traffic websites, the advantage of avoiding network round trips, the single-writer constraint, and the conditions under which a client/server database is preferable.
- [NIST CSRC Glossary — attack surface](https://csrc.nist.gov/glossary/term/attack_surface)
  Used for the definition of “attack surface” applied in this article.
- [AWS — Shared Responsibility Model](https://aws.amazon.com/compliance/shared-responsibility-model/)
  Used to verify the division of responsibility between a cloud provider and its customer, and how the customer's responsibilities vary with the services selected.

### Change Management, Permissions, and Auditing

- [HashiCorp — terraform plan command](https://developer.hashicorp.com/terraform/cli/commands/plan)
  Used to explain how a change plan is derived from remote objects, state, and configuration differences and can be reviewed before it is applied.
- [AWS CloudFormation — Update stacks using change sets](https://docs.aws.amazon.com/AWSCloudFormation/latest/UserGuide/using-cfn-updating-stacks-changesets.html)
  Used to verify that additions, modifications, and removals of resources can be reviewed before execution, and that a change set does not guarantee a successful update.
- [AWS IAM — Security best practices in IAM](https://docs.aws.amazon.com/IAM/latest/UserGuide/best-practices.html)
  Used for guidance on temporary credentials, least privilege, policy conditions, and permission guardrails.
- [AWS CloudTrail — What Is AWS CloudTrail?](https://docs.aws.amazon.com/awscloudtrail/latest/userguide/cloudtrail-user-guide.html)
  Used to verify that operations performed through the console, CLI, SDKs, and APIs can be recorded as events for auditing and analysis.

### System Design and Simplicity

- [Google SRE Workbook — Chapter 7: Simplicity](https://sre.google/workbook/simplicity/)
  The source for the argument that simple systems are easier to understand, maintain, test, and repair, and that simplicity should be treated as an end-to-end objective from architecture through operational processes.
