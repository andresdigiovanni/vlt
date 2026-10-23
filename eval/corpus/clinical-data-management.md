---
title: Clinical data management
source: https://en.wikipedia.org/wiki/Clinical_data_management
revision: 1347375491
license: CC BY-SA 4.0
---

# Clinical data management

**Clinical data management** (**CDM**) is a critical process in clinical research, which leads to generation of high-quality, reliable, and statistically sound data from [[clinical-trial|clinical trials]]. Clinical data management ensures collection, integration and availability of data at appropriate quality and cost. It also supports the conduct, management and analysis of studies across the spectrum of clinical research as defined by the National Institutes of Health (NIH). The ultimate goal of CDM is to ensure that conclusions drawn from research are well supported by the data. Achieving this goal protects public health and increases confidence in marketed therapeutics.

## Role of the clinical data manager in a clinical trial

Job profile acceptable in CDM: clinical researcher, clinical research associate, clinical research coordinator etc. The clinical data manager plays a key role in the setup and conduct of a [[clinical-trial|clinical trial]]. The data collected during a clinical trial form the basis of subsequent safety and efficacy analysis which in turn drive decision making on product development in the pharmaceutical industry. The clinical data manager is involved in early discussions about data collection options and then oversees development of data collection tools based on the clinical trial protocol. Once subject enrollment begins, the data manager ensures that data are collected, validated, complete, and consistent. The clinical data manager liaises with other data providers (e.g. a central laboratory processing blood samples collected) and ensures that such data are transmitted securely and are consistent with other data collected in the clinical trial. At the completion of the clinical trial, the clinical data manager ensures that all data expected to be captured have been accounted for and that all data management activities are complete. At this stage, the data are declared final (terminology varies, but common descriptions are "Database Lock", “Data Lock” and "Database Freeze"), and the clinical data manager transfers data for statistical analysis.

## Pre-Requisites

### Standard operating procedures

Standard operating procedures (SOPs) describe the process to be followed in conducting data management activities and support the obligation to follow applicable laws and guidelines (e.g. ICH [[good-clinical-practice|GCP]] and 21CFR Part 11) in the conduct of data management activities.

### Data management plan

The data management plan describes the activities to be conducted in the course of processing data. Key topics to cover include the SOPs to be followed, the clinical data management system (CDMS) to be used, description of data sources, data handling processes, data transfer formats and process, and quality control procedure

### Case report form design

The case report form (CRF) is the data collection tool for the [[clinical-trial|clinical trial]] and can be paper or electronic. Paper CRFs will be printed, often using No Carbon Required paper, and shipped to the investigative sites conducting the clinical trial for completion after which they are couriered back to Data Management. Electronic CRFs enable data to be typed directly into fields using a computer and transmitted electronically to Data Management. Design of CRFs needs to take into account the information required to be collected by the clinical trial protocol and intended to be included in statistical analysis. Where available, standard CRF pages may be re-used for collection of data which is common across most clinical trials e.g. subject demographics. Apart from CRF design, electronic trial design also includes edit check programming. Edit checks are used to fire a query message when discrepant data is entered, to map certain data points from one CRF to the other, to calculate certain fields like Subject's Age, BMI etc.. Edit checks help the investigators to enter the right data right at the moment data is entered and also help in increasing the quality of the Clinical trial data.

### Database design and build

For a [[clinical-trial|clinical trial]] utilizing an electronic CRF, database design and CRF design are closely linked. The electronic CRF enables entry of data into an underlying relational database. For a clinical trial utilizing a paper CRF, the relational database is built separately. In both cases, the relational database allows entry of all data captured on the Case report form.

### Computerized system validation

All computer systems used in the processing and management of [[clinical-trial|clinical trial]] data must undergo validation testing to ensure that they perform as intended and that results are reproducible.

### CDISC

The [[clinical-data-interchange-standards-consortium|Clinical Data Interchange Standards Consortium]] leads the development of global, system independent data standards which are now commonly used as the underlying data structures for [[clinical-trial|clinical trial]] data. These describe parameters such as the name, length and format of each data field (variable) in the relational database.

### Validation rules

Validation Rules are electronic checks defined in advance which ensure the completeness and consistency of the [[clinical-trial|clinical trial]] data.

### User acceptance testing

Once an electronic CRF (eCRF) is built, the clinical data manager (and other parties as appropriate) conducts User Acceptance Testing (UAT). The tester enters test data into the e-CRF and record whether it functions as intended. UAT is performed until all the issues (if found) are resolved.

## Active Phase

### Data entry

When an electronic CRF is in use, data entry is carried out at the investigative site where the [[clinical-trial|clinical trial]] is conducted by site staff who have been granted appropriate access to do so.

When using a paper CRF the pages are entered by data entry operators. Best practice is for a first pass data entry to be completed followed by a second pass or verification step by an independent operator. Any discrepancies between the first and second pass may be resolved such that the data entered is a true reflection of that recorded on the CRF. Where the operator is unable to read the entry the clinical data manager should be notified so that the entry may be clarified with the person who completed the CRF.

### Data validation

Data validation is the application of validation rules to the data. For electronic CRFs the validation rules may be applied in real time at the point of entry. Offline validation may still be required (e.g. for cross checks between data types)

### Data queries

Where data entered does not pass validation rules then a data query may be issued to the investigative site where the [[clinical-trial|clinical trial]] is conducted to request clarification of the entry. Data queries must not be leading (i.e. they must not suggest the correction that should be made). For electronic CRFs only the site staff with appropriate access may modify data entries. For paper CRFs, the clinical data manager applies the data query response to the database and a copy of the data query is retained at the investigative site. When an item or variable has an error or a query raised against it, it is said to have a “discrepancy” or “query”.

All [[electronic-data-capture|EDC]] systems have a discrepancy management tool or also refer to “edit check” or “validation check” that is programmed using any known programming language (e.g. SAS, PL/SQL, C#, SQL, Python, etc).

So what is a ‘query’? A query is an error generated when a validation check detects a problem with the data. Validation checks are run automatically whenever a page is saved “submitted” and can identify problems with a single variable, between two or more variables on the same eCRF page, or between variables on different pages. A variable can have multiple validation checks associated with it.

Errors can be resolved in several ways:

- by correcting the error – entering a new value for example or when the datapoint is updated

- by marking the variable as correct – some EDC systems required additional response or you can raise a further query if you are not satisfied with the response

### Central laboratory data

Samples collected during a [[clinical-trial|clinical trial]] may be sent to a single central laboratory for analysis. The clinical data manager liaises with the central laboratory and agrees data formats and transfer schedules in Data Transfer Agreement. The sample collection date and time may be reconciled against the CRF to ensure that all samples collected have been analysed.

### Other external data

Analysis of [[clinical-trial|clinical trial]] data may be carried out by laboratories, image processing specialists or other third parties. The clinical data manager liaises with such data providers and agree data formats and transfer schedules. Data may be reconciled against the CRF to ensure consistency.

### Serious adverse event reconciliation

The CRF collects [[adverse-event|adverse events]] reported during the conduct of the [[clinical-trial|clinical trial]] however there is a separate process which ensures that serious adverse events are reported quickly. The clinical data manager must ensure that data is reconciled between these processes.

### Patient recorded data

Where the subject is required to record data (e.g. daily symptoms) then a diary is provided for completion. Data management of this data requires a different approach to CRF data as, for example, it is generally not practical to raise data queries. Patient diaries may be developed in either paper or electronic (eDiary) formats. Such eDiaries generally take the form of a handheld device which enables the subject to enter the required data and transmits this data to a centralised server.

## Database finalization and extraction

Once all expected data is accounted for, all data queries closed, all external data received and reconciled and all other data management activities complete the database may be finalized.

Typical reports generated and used by the clinical data manager includes:

- Status of page completion / missing pages
- Status of data queries
- Data queries not resolved within specified time limit
- Commonly raised data queries (to help identify areas where improvements can be made)

Quality Control is applied at various stages in the Clinical data management process and is normally mandated by SOP.

## Good clinical data management practice

Good clinical data management practice (GCDMP) is the current industry standards for clinical data management that consist of best business practice and acceptable regulatory standards. In all phases of [[clinical-trial|clinical trials]], clinical and laboratory information must be collected and converted to digital form for analysis and reporting purposes. The [[food-and-drug-administration|U.S. Food and Drug Administration]] and International Conference on Harmonisation of Technical Requirements for Registration of Pharmaceuticals for Human Use have provided specific regulations and guidelines surrounding this component of the drug and device development process. The effective, efficient and regulatory-compliant management of clinical trial data is an essential component of drug and device development.

The Society for Clinical Data Management (SCDM) has created the Good Clinical Data Management Practices (GCDMP©) standard, a comprehensive document that provides guidance on accepted practices of clinical data management (CDM) that are not totally covered by current guidelines and regulations. This document is updated by Subject Matter Experts on a continuous basis.

## Professional organizations for clinical data management

- International Network of Clinical Data Management Associations (INCDMA) aims at the promotion of collaboration among clinical data management groups around the world. It is an international forum for discussion of and feedback on current topics of relevance to the discipline of CDM. It is composed of members the boards of the SCDM, ACDM, DMB (France), PSDM (The Netherlands) who participate in the INCDMA proceedings and funding. It also regroups DM leaders and subject matter experts from Europe, North America, Israel, Japan, China and Australia.
- The Association for Clinical Data Management (ACDM) is a global organization founded in 1987 and is Empowering Excellence in Clinical Data Management. The ACDM represents a vibrant community of dedicated clinical research professionals who are passionate about enhancing the [[drug-development|drug development]] process through fostering an environment that encourages continuous learning and the practical application of modern Clinical Data Management practices. With over 35 years of service to the data management community, the ACDM's unwavering commitment is to provide clear and effective platforms for its members to network, share experiences, and stay abreast of the latest regulations and standards in Clinical Data Management. Along with supporting members on their professional development journeys, offering opportunities to gain the skills and insights needed to excel in their careers and shape the future of data management.
- The Society for Clinical Data Management (SCDM) is an international organization with 2,590 members promoting quality and excellence in data management, and featuring webinars, online courses, certification, and an annual conference.
- The Association Française de Data Management Biomédicale (DMB) is a French data management organization created in 1995. Gathers data from data managers (anyone involved in data management activity for developing drugs) from the pharmaceutical industry, [[contract-research-organization|CROs]], software vendors or universities.
- The French network of data managers in academic biomedical research (AcaDM) is a network founded in 2008 which aims to offer a space for reflection between experts in order to standardize and improve practices.
