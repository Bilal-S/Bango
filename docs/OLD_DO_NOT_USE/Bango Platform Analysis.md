# **Bango Platform Analysis**

## **Platform Overview**

Bango is a specialized, cloud-based platform designed specifically to streamline and accelerate the initial stages of systematic literature reviews, scoping reviews, and meta-analyses. Originally developed by researchers at the Qatar Computing Research Institute (QCRI), Bango emerged as a solution to the cumbersome, error-prone traditional methods of screening articles using spreadsheet software. By offering a dedicated interface on both web browsers and mobile applications, Bango has transitioned the systematic review process into a modernized, collaborative, and highly portable workflow. Today, it serves a global user base of academics, medical researchers, and students, operating primarily on a freemium model that balances basic accessibility with advanced premium capabilities.

## **Core Features**

### **Data Import and Aggregation**

The foundation of any systematic review is the initial literature search, which often spans multiple databases such as PubMed, Scopus, Web of Science, and Embase. Bango simplifies this by allowing users to upload large reference files directly into a new project. The system seamlessly parses standard bibliographic file formats, including RIS, CSV, CIW, and PubMed XML formats. By integrating smoothly with major citation managers like Zotero, Mendeley, and EndNote, Bango acts as a central repository. It is built to handle massive datasets, effortlessly parsing metadata—such as authors, publication years, journal titles, and abstracts—for tens of thousands of references without severely degrading browser performance.

**User Story:** As a principal investigator, I want to upload multiple RIS files exported from PubMed and Scopus directly into a single Bango project, so my entire research team has a centralized database of all potential literature to begin screening.

### **Intelligent Deduplication**

Because researchers must query multiple databases to ensure comprehensive coverage, importing duplicate articles is inevitable. Finding and removing these duplicates manually is notoriously tedious. Bango addresses this by deploying an advanced natural language processing algorithm to identify overlapping records. The system does not merely look for exact matches; it uses fuzzy logic to group records with slight discrepancies in author formatting or publication data. It then presents a confidence percentage for each grouped match. Users are prompted to manually review and resolve these potential duplicates, ensuring that valuable unique studies are not accidentally discarded while saving countless hours of manual cross-referencing.

**User Story:** As a reviewer managing a 5,000-article dataset, I want the system to automatically group potential duplicate papers and highlight their similarities, so I can click "delete" on the copies and avoid reading the same abstract multiple times.

### **Optimized Screening Workflow**

The core functionality of Bango lies in its ergonomic screening interface. Users are presented with a split-screen or dynamic view where they can rapidly read titles and abstracts to make their primary triage decisions. Researchers simply click "Include," "Exclude," or "Maybe." To further accelerate the process, the web interface supports customized keyboard shortcuts.

Recognizing that researchers often work on the go, Bango’s mobile app introduces a highly intuitive, Tinder-style swipe interface—swipe right to include, swipe left to exclude, and swipe up for maybe. Crucially, the mobile app allows researchers to download batches of articles and work entirely offline, syncing their decisions with the cloud database once an internet connection is re-established.

**User Story:** As a busy medical resident, I want to use the Bango mobile app in offline mode to swipe through abstracts during my train commute, so I can continuously make screening decisions without needing a laptop or an active internet connection.

### **Collaboration Tools and Blinding**

Systematic reviews demand rigorous methodology, which typically requires at least two independent researchers to screen every article to minimize human error and bias. Bango facilitates this by allowing project owners to invite collaborators and assign specific access roles (e.g., viewer, collaborator, or translator).

A critical feature in this collaborative environment is "Blind Mode." When activated by the project owner, this mode completely hides the screening decisions of other team members, adhering to the strict methodological standards set by organizations like Cochrane. Once the independent screening phase is complete, the owner toggles Blind Mode off. Bango then automatically generates a filtered list of "Conflicts"—articles where reviewers disagreed. Teams can then use integrated chat features and internal notes to discuss and resolve these discrepancies to reach a final consensus.

**User Story:** As a project manager, I want to enable "Blind Mode" before inviting my two research assistants to screen articles, so I can guarantee their inclusion and exclusion decisions are made independently without being influenced by each other's choices.

### **Machine Learning and Predictive Analytics**

Bango’s most distinctive advantage is its background machine learning engine, which relies on active learning models like Support Vector Machines (SVM). As users begin screening and logging their include/exclude decisions, the algorithm continuously learns the specific criteria and language the researchers value.

The tool computes a relevance rating for the remaining unscreened articles, assigning them a predictive five-star score. Researchers can use this to dynamically reorder their screening queue, front-loading the most highly relevant articles. In massive reviews, this allows teams to potentially stop screening early when remaining articles consistently fall to a 1-star rating. Additionally, the system automatically highlights key inclusion terms in green and exclusion terms in red within the abstract text, creating a visual shorthand that drastically reduces cognitive load.

**User Story:** As a researcher facing a massive backlog of unscreened papers, I want to sort my remaining articles by the system's 5-star predictive relevance rating, so I can review the most likely candidates for inclusion first and accelerate my workflow.

### **Organization, Faceting, and Filtering**

To manage the complexity of thousands of articles, researchers can apply custom labels and tags to categorize studies by methodology, region, or specific topics. Furthermore, Bango mandates that researchers log specific reasons for excluding studies (e.g., "Wrong population," "Wrong intervention"), which is a strict requirement for academic reporting.

The interface features a powerful faceting sidebar that acts as a dynamic filter. Users can instantly slice their data by utilizing the PICO framework—filtering searches by Population, Intervention, Comparison, and Outcome. They can also filter by publication year, specific authors, or language, allowing teams to systematically divide the workload or isolate specific subsets of data for secondary analysis.

**User Story:** As a team collaborator, I want to filter the project dashboard by the specific exclusion reason "Wrong Intervention," so I can double-check our criteria consistency and ensure no valid studies were accidentally tossed out.

### **Exporting and PRISMA Reporting**

Once the title and abstract screening phase concludes, teams must export their finalized dataset for the full-text extraction phase. Bango allows users to export the filtered lists—complete with all attached labels, decisions, and exclusion reasons—back into CSV, Word, or RIS formats.

Critically, the software meticulously tracks the numerical flow of data throughout the project lifecycle. It records exactly how many articles were imported, how many duplicates were purged, how many were screened, and how many were excluded with specific reasons. Researchers rely heavily on this precise audit trail to automatically populate PRISMA (Preferred Reporting Items for Systematic Reviews and Meta-Analyses) flow diagrams, which are mandatory visual figures required for the publication of systematic reviews in peer-reviewed journals.

**User Story:** As a lead author preparing a manuscript, I want to export a summary report of exact screening numbers and specific exclusion reasons, so I can easily populate my PRISMA flow diagram for final journal submission.

## **Technical Implementation**

Bango operates on a highly scalable, cloud-based architecture designed to handle intensive, concurrent data processing. Developers built the core web application using the Ruby on Rails framework, known for its rapid development capabilities and robust handling of complex relational data. The platform is hosted across Heroku and Amazon Web Services (AWS), allowing the infrastructure to automatically scale its computing resources up or down based on fluctuating global web traffic and processing demands.

Permanent relational data—such as user accounts, project metadata, and specific article tags—is securely stored in a PostgreSQL database. However, because searching through millions of text-heavy abstracts using standard SQL would be too slow, Bango utilizes Apache Solr. Solr provides blazing-fast, enterprise-level text indexing, which powers the platform's instantaneous search bar and dynamic faceting sidebar.

Heavy computational tasks are decoupled from the main web application to ensure the user interface never freezes. Background worker queues (typically utilizing tools like Sidekiq and Redis) handle intensive asynchronous processing. When a user uploads a massive RIS file, initiates the duplicate identification algorithm, or when the machine learning models need to recalculate the 5-star relevance predictions based on new user decisions, these jobs run quietly in the background. This architecture ensures that the complex predictive analytics continuously update without interrupting the user's workflow. Furthermore, rigorous data security measures, automated backups, and encrypted connections are implemented to protect sensitive, unpublished academic research.