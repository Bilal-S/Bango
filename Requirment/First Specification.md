# **/speckit.specify: Success Criteria**

* The system successfully deduplicates all imported articles.  
* The system categorizes every unique article into either the rejected list or the included list.  
* The system exports the finalized included list in a valid RIS format.  
* The user successfully imports the exported RIS file directly into Zotero.

# **/speckit.specify: Functional Requirements**

## **Core Workflow**

1. The app imports existing RIS bibliography files. The system parses standard metadata fields. These fields include Title, Abstract, Authors, Year, and DOI. The app handles large files without freezing the user interface.  
2. The app runs a deduplication process to find and remove duplicate records. The system matches records based on title, publication year, and authors. It groups suspected duplicates. It auto-merges exact matches. It flags partial matches for user review.  
3. The app stores articles in four distinct lists: imported, working, rejected, and included. These lists represent strict database states. Articles move between states through explicit user action or AI processing.  
4. The user inputs research aims, inclusion criteria, and exclusion criteria. The app stores these as discrete text strings.  
5. The user sets priority levels for criteria: critical, high, moderate, low, or optional. These priorities act as weights during the AI evaluation phase.  
6. The app suggests relevant tags based on RIS metadata and user criteria. The system scans article abstracts to recommend these predefined categories.  
7. The user configures connections to hosted LLMs (OpenAI, Google, z.ai) or local setups (llama.cpp, Ollama, LM Studio). The user provides API keys and endpoint URLs. The app securely encrypts these credentials in local storage.  
8. The AI scans the working list, especially the Topics in RIS imports, to develop tags for accepted or rejected articles. The user can manually edit or delete these generated tags.  
9. The AI screens the working list against the defined criteria and research aims. The app analyzes each article in isolation. The AI evaluates only the abstract. The app does not process full texts. The system processes articles in background batches to manage API rate limits.  
10. The AI prompts enforce strict data formatting. The LLM must return all screening decisions, tags, and reasoning exclusively as structured JSON.  
11. The AI assigns a reasoning paragraph, matching criteria tags, and labels to each article based on the JSON response. The reasoning paragraph cites specific sentences from the abstract to justify the decision.  
12. The AI moves processed articles into the rejected list or included list.  
13. The user manually edits tags, labels, and list states. The user can override any AI decision and manually move an article back to the working list.  
14. The app exports the included list in RIS format. The export includes all original metadata plus the AI-generated tags and reasoning notes.  
15. The AI writes an overall summary of included articles. This summary identifies research trends, assesses methodological strengths, and highlights common weaknesses across the included studies.  
16. The app generates a visual PRISMA 2020 flow diagram. This diagram displays exact record counts at each phase (identification, screening, included). Users can export this diagram as an image file.  
17. The app imports and exports complete project data in JSON format. This allows users to share full project states with collaborators.

## **Logical Resolutions**

* Rule Conflict: A higher priority rule always outweighs a lower priority rule. If an article triggers a "high" priority inclusion and a "moderate" priority exclusion, the system includes the article.  
* Priority Tie: If an article triggers an inclusion criterion and an exclusion criterion of equal priority, the system includes the article.  
* State Flow: Imported \-\> Deduplicated \-\> Working \-\> AI Review \-\> Included OR Rejected. An article can only exist in one state at any given time.

# **/speckit.specify: Non-Functional Requirements**

## **AI Context and Limitations**

* The system requires an LLM with a context window of 50,000 tokens or larger.

## **Hardware Requirements**

* The app does not enforce strict hardware blocks.  
* The app queries the local system resources. The system displays a warning message if the user selects a local AI configuration and the host machine has less than 16GB of VRAM.

## **Database Limits**

* The app stores all project data in a local SQLite database.  
* The system monitors database file size and row counts. The app issues a UI warning if the data reaches 80% of maximum SQLite limits.

# **/speckit.specify: User Stories and Acceptance Criteria**

**Story 1: Data Import** As a Researcher, I want to import RIS files so I can add articles to my project.

* Given I have a valid RIS file.  
* When I upload the file to the app.  
* Then the app parses all metadata and populates the imported list.  
* Given I have an invalid or corrupted file.  
* When I attempt to upload it.  
* Then the app displays a specific parsing error and rejects the upload.

**Story 2: Deduplication** As a Researcher, I want to remove duplicates so I do not review the same text twice.

* Given I have a populated imported list.  
* When I run the deduplication workflow.  
* Then the app compares title, year, and authors to find exact duplicate entries and moves unique articles to the working list.  
* Then the app presents a side-by-side comparison view for fuzzy matches requiring manual user confirmation.

**Story 3: AI Configuration** As a Researcher, I want to connect local or hosted LLMs so the app can process text.

* Given I have API credentials or a local AI server running.  
* When I enter the connection details and click "Test Connection".  
* Then the app pings the endpoint, verifies the response, and saves the configuration locally.  
* Then the app displays a VRAM warning if a local provider is selected on a machine with under 16GB VRAM.

**Story 4: AI Screening** As a Researcher, I want the AI to screen the working list so I save time on manual review.

* Given I have articles in the working list and active criteria.  
* When I trigger the AI screening process.  
* Then the AI isolates the abstract and evaluates it against the criteria.  
* Then the AI returns a structured JSON payload containing the decision, reasoning note, and tags.  
* Then the app parses the JSON and moves the article to the correct final list.

**Story 5: Project Export** As a Researcher, I want to export the entire project so I can back up my data.

* Given I have an active project with articles, lists, and criteria.  
* When I click export project.  
* Then the app generates a single JSON file containing all project settings, LLM configurations, and current article states.

# **/speckit.plan: Domain Model**

{  
  "Project": {  
    "id": "string",  
    "name": "string",  
    "researchAims": "string",  
    "createdAt": "timestamp",  
    "lastModified": "timestamp",  
    "criteria": \["Criteria"\],  
    "tags": \["Tag"\],  
    "articles": \["Article"\],  
    "llmConfig": "LLMConfig"  
  },  
  "Criteria": {  
    "id": "string",  
    "type": "enum\[inclusion, exclusion\]",  
    "text": "string",  
    "priority": "enum\[critical, high, moderate, low, optional\]"  
  },  
  "Article": {  
    "id": "string",  
    "title": "string",  
    "abstract": "string",  
    "authors": \["string"\],  
    "publicationYear": "integer",  
    "doi": "string",  
    "risData": "object",  
    "status": "enum\[imported, working, included, rejected\]",  
    "aiReasoning": "string",  
    "appliedCriteria": \["string"\],  
    "tags": \["string"\],  
    "labels": \["string"\]  
  },  
  "LLMConfig": {  
    "provider": "enum\[openai, google, z.ai, llama.cpp, ollama, lm\_studio\]",  
    "endpoint": "string",  
    "apiKey": "string",  
    "modelName": "string",  
    "temperature": "float"  
  }  
}

# **/speckit.plan: Technology Stack**

* **Framework**: Tauri 2.x. This provides a lightweight binary. It enables cross-platform desktop and mobile support without heavy browser overhead.  
* **Frontend**: Vue 3.x (TypeScript). This ensures strict type-checking for complex UI state management during screening.
* **Backend**: Rust. This ensures memory safety. It handles intensive background tasks like RIS parsing and deduplication without blocking the user interface.  
* **Database**: Local SQLite. This guarantees data portability and supports offline functionality. It handles standard review datasets easily.  
* **AI Integration**: Direct REST API client in Rust. This handles asynchronous requests to external or local LLM endpoints.