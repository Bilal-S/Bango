Similar to Bango Platform I want to create a desktop and mobile application based on Tauri 2.x platform that does the following:

The objective is to review academic articles for inclusion and exclusion using AI

- it can import existing RIS bibliography files with all details
- it allows users to enter a list of inclusion criteria and priorities
- it allows users to enter a list of exclusion criteria
- it allows the user to enter a list of research aims
- priorities tags are: critical, high, moderate, low, optional
- the app should suggest a list of tags based on the overall RIS article categories, inclusion and exclusion criteria


- the app should scan working list meta data, inclusion and exclusion criteria to develop meta tags that can be applied to articles that are accepted or rejected. User should be able to change the entries


- the main process should iterate through all provided sources and do the following
	- determine if a source can be included, excluded based on research aims and inclusion or exclusion criteria and ensure that articles are tagged for inclusion and exclusion
	- for priorities when there is a disagreement an inclusion criterion with same priority is more relevant that an exclusion criterion. i.e. when in doubt include
	- add notes to each article for overall reason for your decision including. Write out reasoning paragraph.
	- tag each article with inclusion and exclusion criteria that match
	- tag each article with labels that match

- allow users to change tags and labels on articles
- allow users to move articles between 


- the app should allows the user to enter connection to hosted LLMs like openai, google, and z.ai and local ai compatible setups such as llama.cpp, Ollama, and LM Studio


- the app should have an initial de-deduplication workflow, where duplicates are matched and removed
- the app needs to maintain separate lists to indicate process status
	- imported list: the original imported RIS articles
	- working list: the deduplicated list of articles that has not further reviewed
	- the rejected list: after review the article has been rejected
	- the included list: after review the article has been included

- the main work process iterates through working list and moves articles to either rejected or included list	

- the app should allow export in RIS format of included list

- the app should offer an overall AI summary from abstracts of included articles and main theme that has emerged vis-a-vis the research objectives. Connect and identify trends, assess strength and weaknesses of previous research

- app should create a PRISMA diagram (PRISMA 2020) showing the process status 


- The app should allow a complete export and import of a project with all articles, list items and settings of the project. This can be in any compact data format including JSON.