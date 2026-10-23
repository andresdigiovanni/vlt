---
title: Clinical Data Interchange Standards Consortium
source: https://en.wikipedia.org/wiki/Clinical_Data_Interchange_Standards_Consortium
revision: 1374995265
license: CC BY-SA 4.0
---

# Clinical Data Interchange Standards Consortium

The **Clinical Data Interchange Standards Consortium** (**CDISC**) is a standards developing organization (SDO) dealing with medical research data linked with healthcare, made to enable information system interoperability and to improve medical research and related areas of healthcare. The standards support medical research from protocol through analysis and reporting of results and have been shown to decrease resources needed by 60% overall and 70–90% in the start-up stages when they are implemented at the beginning of the research process. Since December 2016, CDISC standards are mandatory for submission to US FDA.

CDISC standards are harmonized through a model that is also a HL7 standard and is the process to becoming an ISO/CEN standard.

## History

- Late 1997 – Started as a Volunteer group
- Summer 1998 – Invited to form DIA SIAC
- 1999 – SDS v1.0; ODM v0.8
- 2000 – SDS v1.1
- Feb 2000 – Formed an Independent, non-profit organization
- Dec 2001 – Global participation
- 2001 – SDS v2.0; ODM v1.0
- 2002 – ODM v1.1; ADAM v1.0
- 2003 – LAB v1.0; SDTM v1/SDTM-IG v3.0; BRIDG Model Initiated; SEND 1.0
- 2004 – LAB v1.1; ODM v1.2; SDTM v3.1
- 2005 – Define.xml Implementation; Release (v1.0); SEND v.2; ODM v1.2.1; SDTM v1.1/SDTMIG v3.1.1; ODM mapped to HL7 RIM
- 2006 – BRIDG v1.0, v1.1; BRIDG posted as open-source model
- 2007 – ODM v1.3; LAB & SDTM Aligned; BRIDG posted as open-source model
- 2008 – BRIDG v2.0, v2.1, v2.2; CDASH v1.0; ESDI document published
- 2009 – SDTM v1.2/SDTMIG 3.1.2; ADAM v2.1 and ADAMIG v1.0; Imaging CRFs; CDISC-IHE RFD and RPE
- 2010 – Protocol Representation Model;(PRM) v1.0; BRIDG v3.0; ODM v1.3.1; HHS-ONC/HITSP Interoperability Specification #158; CDISC-IHE RPE
- 2011 – CDASH v1.1; SEND v3.0; Study Design XML v1.0, ADAM Examples in Commonly Used Statistical Analysis Methods v1.0
- 2012 – The ADAM Basic Data Structure for Time-to-Event Analyses v1.0, ADAM Data Structure for Adverse Event Analysis v1.0
- 2013 – Define-XML Version 2.0, SDTM v1.4/SDTMIG v3.2
- 2014 – SHARE R1
- 2015 – ARM v1.0 extension for Define-XML Version 2
- 2016 – Dataset XML, Therapeutic Areas, CTR-XML, ADAMIG v1.1, ADM OCCDS v1.0

## Overview of standards

- Dataset.XML (DataSet-XML)
  - Enables communication of study results as well as regulatory submission to [[food-and-drug-administration|FDA]] (pilot since 2014).
- Study Data Tabulation Model (SDTM)
  - Recommended for FDA regulatory submissions since 2004.
  - The SDTM Implementation Guide (SDTM-IG) gives a standardized, predefined collection of domains for clinical data submission, each of which is based on the structure and metadata defined by the SDTM.
- Standard for Exchange of Non-clinical Data (SEND)
  - The SEND Implementation Guide (SEND-IG) provides predefined domains and examples of non-clinical (animal) data based on the structure and metadata defined by the SDTM.
- Analysis Data Model (ADaM)
  - Defines dataset and metadata standards that support statistical analyses and traceability. ADaM is one of the required standards for data submission to FDA (U.S.) and PMDA (Japan).
- Operational Data Model (ODM)
  - The highlights of ODM: includes an audit trail, utilizes XML technology, machine- and human-readable, all information are independent of databases, storing of ODM is independent of hard- and software.
- Laboratory Data Model (LAB)
  - The Lab standard is used for the exchange of laboratory data between labs and [[contract-research-organization|CROs]]
- Case Report Tabulation Data Definition Specification (CRT-DDS)
  - Also referred to as "define.xml", a machine readable version of the regulatory submission "define.pdf".
- Clinical Data Acquisition Standards Harmonization (CDASH)
  - Defines a minimal data collection set for sixteen safety SDTM Domains, harmonizing element names, definitions and metadata. The objective is to establish a standardized data collection baseline across all submissions.
- CDISC Terminology
  - Defines controlled terminology for SDTM and CDASH, provides extensible lists of controlled terms designed to harmonize data collected across submissions.

## Individual standards

### Operational Data Model (ODM)

The CDISC Operational Data Model (ODM) is designed to facilitate the regulatory-compliant acquisition, archive and interchange of metadata and data for clinical research studies. ODM is a vendor-neutral, platform-independent format for interchange and archive of clinical study data. The model includes the clinical data along with its associated metadata, administrative data, reference data and audit information. ODM was first introduced in 1999, and the latest version, 1.3.2, was released in 2012. ODM extensions have been developed to create a number of additional CDISC standards, including Define-XML, Dataset-XML, SDM-XML, and CTR-XML and future planned standard Protocol-XML.

ODM is an XML based standard and it is an XML schema that provides a number of constructs for modelling electronic Case Report Forms (CRFs). ODM is often combined with the Study Data Model standard to more fully model trial arms or trial activities. ODM is also used in sending forms data from a [[clinical-trial|clinical trial]] system to an electronic health record (EHR) system.

The ODM schema is generally divided into three categories of data: Metadata, Admin data, and Clinical data. Metadata describes the structure of the eCRFs within the study, and how they relate to scheduled visits. Admin data contains references to users, locations, and any additional non-structural and non-clinical reference data. Clinical data contains all eCRF item values and references both Metadata and Admin data.

### Define-XML

Define-XML supports the interchange of dataset metadata for clinical research applications in a machine-readable format. An important use case for Define-XML is to support the submission of [[clinical-trial|clinical trials]] data in CDISC SDTM, SEND or ADaM format to regulatory authorities. The key metadata components to support submissions are:

- Dataset definition
- Dataset variable definitions
- Controlled Terminology definitions
- Value list definitions
- Links to supporting documents
- Computational method definitions
- Comments definitions

Define-XML can also be used to describe proprietary, non-CDISC dataset structures. The Define-XML model is implemented using extensions to the CDISC Operational Data Model (ODM) XML schema. The current version is 2.0 published on the CDISC website.

### CTR-XML

[[clinical-trial|Clinical Trial]] Representation allows representing basic characteristics of a clinical trial, such as study sponsor, study name, size of the trial (number of participants). The standard was first introduced in 2016.

## BRIDG

CDISC BRIDG model is a unifying model of the domain of clinical research and research studies. It defines basic elements such as investigator, subject, study, intervention. It is used to keep all standards consistent. It was first introduced in 2006 with version 2 released in 2008. It can be obtained as UML model as well as OWL format.

## SHARE

CDISC SHARE (Shared Health and Clinical Research Electronic Library) is a metadata repository that supports the development, governance, publishing, and consumption of CDISC standards in human and machine-readable formats. SHARE helps users find, understand, and use rich metadata (i.e., research concepts, data elements and attributes, the relationship among data elements, properties in a relationship, and controlled terminologies) relevant to clinical studies more efficiently and consistently. With all this information in a single repository, SHARE will improve integration and traceability of clinical data end-to-end, from protocol through analysis. SHARE will provide a collaborative standards development environment that will improve quality, integration, and consistency across CDISC standards.

## CDISC-registered solutions providers

CDISC maintains a list of solutions providers, subject matter experts and consultants deemed to have sufficient knowledge and experience implementing the various CDISC standards.

## ODM and EDC integration

[[electronic-data-capture|Electronic data capture]] (EDC) systems can be certified as compliant with the Operational Data Model (ODM) by CDISC. There are two main types of integration, ODM Import and ODM Export.

### ODM import

Full import allows importing of ODM-formatted clinical data (Metadata and Data). This is useful for setting up the [[electronic-data-capture|EDC]] system to capture data. It basically allows third party software to define the forms, variables etc. used in the EDC system. This provides an EDC vendor-neutral system for defining a study.

### ODM export

The [[electronic-data-capture|EDC]] system will generate ODM data files for further processing. For example, REDCap Cloud data capture systems to allow export of a study in ODM.
