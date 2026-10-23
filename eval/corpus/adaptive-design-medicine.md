---
title: Adaptive design (medicine)
source: https://en.wikipedia.org/wiki/Adaptive_design_(medicine)
revision: 1364723345
license: CC BY-SA 4.0
---

# Adaptive design (medicine)

In an **adaptive design** of a [[clinical-trial|clinical trial]], the parameters and conduct of the trial for a candidate drug or vaccine may be changed based on an interim analysis. Adaptive design typically involves advanced statistics to interpret a clinical trial [[clinical-endpoint|endpoint]]. This is in contrast to traditional single-arm (i.e. non-randomized) clinical trials or [[randomized-controlled-trial|randomized clinical trials]] (RCTs) that are static in their protocol and do not modify any parameters until the trial is completed. The adaptation process takes place at certain points in the trial, prescribed in the trial protocol. Importantly, this trial protocol is set before the trial begins with the adaptation schedule and processes specified. Adaptions may include modifications to: dosage, sample size, drug undergoing trial, patient selection criteria and/or "cocktail" mix. The PANDA (A Practical Adaptive & Novel Designs and Analysis toolkit) provides not only a summary of different adaptive designs, but also comprehensive information on adaptive design planning, conduct, analysis and reporting.

## Purpose

The aim of an adaptive trial is to more quickly identify drugs or devices that have a therapeutic effect, and to zero in on patient populations for whom the drug is appropriate. When conducted efficiently, adaptive trials have the potential to find new treatments while minimizing the number of patients exposed to the risks of [[clinical-trial|clinical trials]]. Specifically, adaptive trials can efficiently discover new treatments by reducing the number of patients enrolled in treatment groups that show minimal efficacy or higher adverse-event rates. Adaptive trials can adjust almost any part of its design, based on pre-set rules and statistical design, such as sample size, adding new groups, dropping less effective groups and changing the probability of being randomized to a particular group, for example.

## History

In 2004, a Strategic Path Initiative was introduced by the United States [[food-and-drug-administration|Food and Drug Administration]] (FDA) to modify the way drugs travel from lab to market. This initiative aimed at dealing with the high attrition levels observed in the clinical phase. It also attempted to offer flexibility to investigators to find the optimal clinical benefit without affecting the study's validity. Adaptive [[clinical-trial|clinical trials]] initially came under this regime.

The FDA issued draft guidance on adaptive trial design in 2010. In 2012, the President's Council of Advisors on Science and Technology (PCAST) recommended that the FDA "run pilot projects to explore adaptive approval mechanisms to generate evidence across the lifecycle of a drug from the pre-market through the post-market phase." While not specifically related to clinical trials, the council also recommended that they "make full use of accelerated approval for all drugs meeting the statutory standard of addressing an unmet need for a serious or life-threatening disease, and demonstrating an impact on a [[clinical-endpoint|clinical endpoint]] other than survival or irreversible morbidity, or on a [[surrogate-endpoint|surrogate endpoint]], likely to predict clinical benefit."

By 2019, the FDA updated their 2010 recommendations and issued "Adaptive Design Clinical Trials for Drugs and Biologics Guidance". In October of 2021, the FDA Center for Veterinary Medicine issued the Guidance Document "Adaptive and Other Innovative Designs for Effectiveness Studies of New Animal Drugs".

## Characteristics

Traditionally, [[clinical-trial|clinical trials]] are conducted in three steps:

1. The trial is designed.
2. The trial is conducted as prescribed by the design.
3. Once the data are ready, they are analysed according to a pre-specified analysis plan.

## Types

### Overview

Any trial design that can change its design, during active enrollment, could be considered an adaptive [[clinical-trial|clinical trial]]. There are a number of different types, and real life trials may combine elements from these different trial types: In some cases, trials have become an ongoing process that regularly adds and drops therapies and patient groups as more information is gained.

| Trial Design Type | Adaptable Element | Description |
| --- | --- | --- |
| Dose-finding | Treatment dose | Dose may be changed to find minimally toxic, and maximally effective dosing. |
| Adaptive hypothesis | Trial endpoints | According to pre-set protocols, these trials can adapt to investigate new hypothesis, and add new endpoints accordingly. An example would be switch from a superiority to a non-inferiority design. |
| Group sequential | Sample size, by a set interval at a time. | Sample sizes can be changed. These trials usually change the sample size by adding or removing set-blocks of patients such as adding 20 patients at a time, and then re-evaluating. This type of design is explained in detail on PANDA. |
| Response adaptive randomisation | Randomization ratios | The chance of being randomized into one particular group, can change. Treatment groups are not added or dropped, but the chance of being randomized, for example, into the treatment group could increase after interim analysis. This type of design is explained in detail on PANDA. |
| Adaptive treatment-switching | Treatment | These trials, based on pre-set rules, can change individual patients from one group to another. |
| Biomarker adaptive | Multiple, on the basis of biomarker discoveries | These trials incorporated biomarkers into their decision making process. Examples including focusing on a sub-population that may be more biological receptive to a treatment, or choosing new treatments for a trial as more becomes known about the biology of the disease. |
| Population enrichment | Population enrolled | The population that the trial enrolls from may change based on, for example, improved epidemiological understanding of a disease. This type of design is explained in detail on PANDA. |
| Platform Trial | Multiple, on the basis that all different treatment groups share the same, single control group | Platform trials are defined by having a constant control group, against which variable treatment groups are compared. |
| Multi-arm multi-stage | The current treatment arms | These trials adapt to stop recruitment to treatment arms that show less efficacy and therefore they do not allocate new participants to the least effective-seeming treatment arms. This type of design is explained in detail on PANDA. |
| Sample size re-estimation | Sample size | Sample sizes of either the whole trial or individual groups may change as more becomes known about effect sizes. This type of design is explained in detail on PANDA. |
| Seamless Phase I/II | Entry into phase II trials | These trials collect data on safety and dosing simultaneously. |
| Seamless Phase II/III | Entry into phase III trials | These trials collect data on dosing and efficacy simultaneously. |

### Dose finding design

[[phases-of-clinical-research|Phase I]] of clinical research focuses on selecting a particular dose of a drug to carry forward into future trials. Historically, such trials have had a "rules-based" (or "algorithm-based") design, such as the 3+3 design. However, these "A+B" rules-based designs are not appropriate for phase I studies and are inferior to adaptive, model-based designs. An example of a superior design is the continual reassessment method (CRM).

### Group sequential design

Group sequential design is the application of sequential analysis to [[clinical-trial|clinical trials]]. At each interim analysis, investigators will use the current data to decide whether the trial should either stop or should continue to recruit more participants. The trial might stop either because the evidence that the treatment is working is strong ("stopping for benefit") or weak ("stopping for futility"). Whether a trial may stop for futility only, benefit only, or either, is stated in advance. A design has "binding stopping rules" when the trial must stop when a particular threshold of (either strong or weak) evidence is crossed at a particular interim analysis. Otherwise it has "non-binding stopping rules", in which case other information can be taken into account, for example safety data. The number of interim analyses is specified in advance, and can be anything from a single interim analysis (a "two-stage" design") to an interim analysis after every participant ("continuous monitoring").

For trials with a binary (response/no response) outcome and a single treatment arm, a popular and simple group sequential design with two stages is the Simon design. In this design, there is a single interim analysis partway through the trial, at which point the trial either stops for futility or continues to the second stage. Mander and Thomson also proposed a design with a single interim analysis, at which point the trial could stop for either futility or benefit.

For single-arm, single-stage binary outcome trials, a trial's success or failure is determined by the number of responses observed by the end of the trial. This means that it may be possible to know the conclusion of the trial (success or failure) with certainty before all the data are available. Planning to stop a trial once the conclusion is known with certainty is called *non-stochastic curtailment*. This reduces the sample size on average. Planning to stop a trial when the probability of success, based on the results so far, is either above or below a certain threshold is called *stochastic curtailment*. This reduces the average sample size even more than non-stochastic curtailment. Stochastic and non-stochastic curtailment can also be used in two-arm binary outcome trials, where a trial's success or failure is determined by the number of responses observed on each arm by the end of the trial.

## Usage

The adaptive design method developed mainly in the early 21st century. In November 2019, the [[food-and-drug-administration|US Food and Drug Administration]] provided guidelines for using adaptive designs in [[clinical-trial|clinical trials]].

### In 2020 COVID-19 related trials

In April 2020, the World Health Organization published an "R&D Blueprint (for the) novel Coronavirus" (Blueprint). The Blueprint documented a "large, international, multi-site, individually [[randomized-controlled-trial|randomized controlled clinical trial]]" to allow "the concurrent evaluation of the benefits and risks of each promising candidate vaccine within 3–6 months of it being made available for the trial." The Blueprint listed a *Global Target Product Profile* (TPP) for COVID‑19, identifying favorable attributes of safe and effective vaccines under two broad categories: "vaccines for the long-term protection of people at higher risk of COVID-19, such as healthcare workers", and other vaccines to provide rapid-response immunity for new outbreaks.

The international TPP team was formed to:

1. Assess the development of the most promising candidate vaccines.
2. Map candidate vaccines and their [[clinical-trial|clinical trial]] worldwide, publishing a frequently-updated "landscape" of vaccines in development.
3. Rapidly evaluate and screen for the most promising candidate vaccines simultaneously before they are tested in humans.
4. Design and coordinate a multiple-site, international randomized controlled trial – the "Solidarity trial" for vaccines – to enable simultaneous evaluation of the benefits and risks of different vaccine candidates under clinical trials in countries where there are high rates of COVID‑19 disease, ensuring fast interpretation and sharing of results around the world.

The WHO vaccine coalition prioritized which vaccines would go into [[phases-of-clinical-research|Phase II]] and III clinical trials, and determined harmonized Phase III protocols for all vaccines achieving the pivotal trial stage.

The global *Solidarity* and European *Discovery* trials of hospitalized people with severe COVID‑19 infection applied adaptive design to rapidly alter trial parameters as results from the four experimental therapeutic strategies emerged. The US National Institute of Allergy and Infectious Diseases (NIAID) initiated an adaptive design, international Phase III trial named the Adaptive COVID-19 Treatment Trial (ACTT) to involve up to 800 hospitalized COVID‑19 people at 100 sites in multiple countries.

### Breast cancer

An adaptive trial design enabled two experimental breast cancer drugs to deliver promising results after just six months of testing, far shorter than usual. Researchers assessed the results while the trial was in process and found that cancer had been eradicated in more than half of one group of patients. The trial, known as I-Spy 2, tested 12 experimental drugs.

#### I-SPY 1

For its predecessor I-SPY 1, 10 cancer centers and the National Cancer Institute (NCI SPORE program and the NCI Cooperative groups) collaborated to identify response indicators that would best predict survival for women with high-risk breast cancer. During 2002–2006, the study monitored 237 patients undergoing neoadjuvant therapy before surgery. Iterative MRI and tissue samples monitored the biology of patients to chemotherapy given in a neoadjuvant setting, or presurgical setting. Evaluating chemotherapy's direct impact on tumor tissue took much less time than monitoring outcomes in thousands of patients over long time periods. The approach helped to standardize the imaging and tumor sampling processes, and led to miniaturized assays. Key findings included that tumor response was a good predictor of patient survival, and that tumor shrinkage during treatment was a good predictor of long-term outcome. Importantly, the vast majority of tumors identified as high risk by molecular signature. However, heterogeneity within this group of women and measuring response within tumor subtypes was more informative than viewing the group as a whole. Within genetic signatures, level of response to treatment appears to be a reasonable predictor of outcome. Additionally, its shared database has furthered the understanding of drug response and generated new targets and agents for subsequent testing.

#### I-SPY 2

I-SPY 2 is an adaptive [[clinical-trial|clinical trial]] of multiple [[phases-of-clinical-research|Phase 2]] treatment regimens combined with standard chemotherapy. I-SPY 2 linked 19 academic cancer centers, two community centers, the [[food-and-drug-administration|FDA]], the NCI, pharmaceutical and biotech companies, patient advocates and philanthropic partners. The trial is sponsored by the Biomarker Consortium of the Foundation for the NIH (FNIH), and is co-managed by the FNIH and QuantumLeap Healthcare Collaborative. I-SPY 2 was designed to explore the hypothesis that different combinations of cancer therapies have varying degrees of success for different patients. Conventional clinical trials that evaluate post-surgical tumor response require a separate trial with long intervals and large populations to test each combination. Instead, I-SPY 2 is organized as a continuous process. It efficiently evaluates multiple therapy regimes by relying on the predictors developed in I-SPY 1 that help quickly determine whether patients with a particular genetic signature will respond to a given treatment regime. The trial is adaptive in that the investigators learn as they go, and do not continue treatments that appear to be ineffective. All patients are categorized based on tissue and imaging markers collected early and iteratively (a patient's markers may change over time) throughout the trial, so that early insights can guide treatments for later patients. Treatments that show positive effects for a patient group can be ushered to confirmatory clinical trials, while those that do not can be rapidly sidelined. Importantly, confirmatory trials can serve as a pathway for FDA Accelerated Approval. I-SPY 2 can simultaneously evaluate candidates developed by multiple companies, escalating or eliminating drugs based on immediate results. Using a single standard arm for comparison for all candidates in the trial saves significant costs over individual Phase 3 trials. All data are shared across the industry. As of January 2016 I-SPY 2 is comparing 11 new treatments against 'standard therapy', and is estimated to complete in Sept 2017. By mid 2016 several treatments had been selected for later stage trials.

### Alzheimer's

Researchers under the European Prevention of Alzheimer's Dementia (EPAD) project by the Innovative Medicines Initiative are utilizing an adaptive trial design to help speed development of Alzheimer's disease treatments, with a budget of 53 million euros. The first trial under the initiative was expected to begin in 2015 and to involve about a dozen companies. As of 2020, 2,000 people over the age of 50 have been recruited across Europe for a long term study on the earliest stages of Alzheimer's. The EPAD project plans to use the results from this study and other data to inform 1,500 person selected adaptive [[clinical-trial|clinical trials]] of drugs to prevent Alzheimer's.

## Bayesian designs

The adjustable nature of adaptive trials inherently suggests the use of Bayesian statistical analysis. Bayesian statistics inherently address updating information such as that seen in adaptive trials that change from updated information derived from interim analysis. The problem of adaptive [[clinical-trial|clinical trial]] design is more or less exactly the bandit problem as studied in the field of reinforcement learning.

According to [[food-and-drug-administration|FDA]] guidelines, an adaptive Bayesian clinical trial can involve:

- Interim looks to stop or to adjust patient accrual
- Interim looks to assess stopping the trial early either for success, futility or harm
- Reversing the hypothesis of non-inferiority to superiority or vice versa
- Dropping arms or doses or adjusting doses
- Modification of the randomization rate to increase the probability that a patient is allocated to the most appropriate treatment (or arm in the multi-armed bandit model)

The Bayesian framework Continuous Individualized Risk Index which is based on dynamic measurements from cancer patients can be effectively used for adaptive trial designs. Platform trials rely heavily on Bayesian designs.

For regulatory submission of Bayesian clinical trial design, there exist two Bayesian decision rules that are frequently used by trial sponsors. First, posterior probability approach is mainly used in decision-making to quantify the evidence to address the question, "Does the current data provide convincing evidence in favor of the alternative hypothesis?" The key quantity of the posterior probability approach is the posterior probability of the alternative hypothesis being true based on the data observed up to the point of analysis. Second, predictive probability approach is mainly used in decision-making is to answer the question at an interim analysis: "Is the trial likely to present compelling evidence in favor of the alternative hypothesis if we gather additional data, potentially up to the maximum sample size (or current sample size)?" The key quantity of the predictive probability approach is the posterior predictive probability of the trial success given the interim data.

In most regulatory submissions, Bayesian trial designs are calibrated to possess good frequentist properties. In this spirit, and in adherence to regulatory practice, regulatory agencies typically recommend that sponsors provide the frequentist type I and II error rates for the sponsor's proposed Bayesian analysis plan. In other words, the Bayesian designs for the regulatory submission need to satisfy the type I and II error requirement in most cases in the frequentist sense. Some exception may happen in the context of external data borrowing where the type I error rate requirement can be relaxed to some degree depending on the confidence of the historical information.

## Statistical analysis

The problem of adaptive [[clinical-trial|clinical trial]] design is more or less exactly the bandit problem as studied in the field of reinforcement learning.

## Added complexity

The logistics of managing traditional, non-adaptive design [[clinical-trial|clinical trials]] may be complex. In adaptive design clinical trials, adapting the design as results arrive adds to the complexity of design, monitoring, drug supply, data capture and randomization. Furthermore, it should be stated in the trial's protocol exactly what kind of adaptation will be permitted. Publishing the trial protocol in advance increases the validity of the final results, as it makes clear that any adaptation that took place during the trial was planned, rather than ad hoc. According to PCAST "One approach is to focus studies on specific subsets of patients most likely to benefit, identified based on validated biomarkers. In some cases, using appropriate biomarkers can make it possible to dramatically decrease the sample size required to achieve statistical significance—for example, from 1500 to 50 patients."

Adaptive designs have added statistical complexity compared to traditional clinical trial designs. For example, any multiple testing, either from looking at multiple treatment arms or from looking at a single treatment arm multiple times, must be accounted for. Another example is statistical bias, which can be more likely when using adaptive designs, and again must be accounted for.

While an adaptive design may be an improvement over a non-adaptive design in some respects (for example, expected sample size), it is not always the case that an adaptive design is a better choice overall: in some cases, the added complexity of the adaptive design may not justify its benefits. An example of this is when the trial is based on a measurement that takes a long time to observe, as this would mean having an interim analysis when many participants have started treatment but cannot yet contribute to the interim results.

## Risks

Shorter trials may not reveal longer term risks, such as a cancer's return.
