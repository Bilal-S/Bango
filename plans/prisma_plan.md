Act as an expert Systematic Literature Review (SLR) methodologist. Your task is to generate a standard PRISMA 2020 flow diagram using Mermaid.js syntax based on the screening data provided below.

1. Study Context

Research Aim: [e.g., To evaluate the impact of blockchain technology on social trust and organizational resilience in supply chains]

Inclusion Criteria: [e.g., Peer-reviewed, published 2018-2026, explicitly utilizes Lewicki’s trust progression stages, empirical data]

Exclusion Criteria: [e.g., Non-English, purely theoretical frameworks without validation, non-supply chain contexts]

2. Search & Screening Data

Records identified from Databases: [Number]

Records identified from Registers: [Number]

Records removed before screening:

Duplicates removed: [Number]

Records removed for other reasons (e.g., automation tools): [Number]

Records screened (Title/Abstract): [Number]

Records excluded: [Number]

Reports sought for retrieval: [Number]

Reports not retrieved: [Number]

Reports assessed for full-text eligibility: [Number]

Reports excluded (with reasons):

Reason 1 (e.g., Wrong context): [Number]

Reason 2 (e.g., Lacked empirical data): [Number]

Reason 3 (e.g., Wrong study design): [Number]

Final studies included in review: [Number]

3. Output Instructions
Generate a complete, top-down Mermaid flowchart (graph TD) strictly adhering to the standard PRISMA 2020 layout.

Structure: Use subgraph syntax to clearly divide the diagram into three distinct vertical phases: Identification, Screening, and Included.

Flow & Branching: Follow the exact logical flow of the PRISMA 2020 statement. Main review steps should flow downwards. Exclusions (Records excluded, Reports not retrieved, Reports excluded with reasons) must branch off to the right of the main flow.

Data Integration: Embed the exact numerical counts into the node labels. For the final exclusion node, list the specific reasons and their respective counts as defined by the inclusion/exclusion criteria data above.

Syntax: Use clean, standard Mermaid syntax. Ensure nodes are properly linked (e.g., A --> B) and use appropriate node shapes (e.g., standard rectangles [text] for all boxes). Do not use HTML tags inside the Mermaid nodes as they can break the renderer.