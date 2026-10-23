---
title: Survival analysis
source: https://en.wikipedia.org/wiki/Survival_analysis
revision: 1379794151
license: CC BY-SA 4.0
---

# Survival analysis

**Survival analysis** is a branch of statistics for analyzing the expected duration of time until one event occurs, such as death in biological organisms and failure in mechanical systems. This topic is called **reliability theory**, **reliability analysis** or reliability engineering in engineering, **duration analysis** or **duration modelling** in economics, and **event history analysis** in sociology. Survival analysis attempts to answer certain questions, such as what is the proportion of a population which will survive past a certain time? Of those that survive, at what rate will they die or fail? Can multiple causes of death or failure be taken into account? How do particular circumstances or characteristics increase or decrease the probability of survival?

To answer such questions, it is necessary to define "lifetime". In the case of biological survival, death is unambiguous, but for mechanical reliability, failure may not be well-defined, for there may well be mechanical systems in which failure is partial, a matter of degree, or not otherwise localized in time. Even in biological problems, some events (for example, heart attack or other organ failure) may have the same ambiguity. The theory outlined below assumes well-defined events at specific times; other cases may be better treated by models which explicitly account for ambiguous events.

More generally, survival analysis involves the modelling of time to event data; in this context, death or failure is considered an "event" in the survival analysis literature – traditionally only a single event occurs for each subject, after which the organism or mechanism is dead or broken. *Recurring event* or *repeated event* models relax that assumption. The study of recurring events is relevant in systems reliability, and in many areas of social sciences and medical research.

## Introduction to survival analysis

Survival analysis is used in several ways:

- To describe the survival times of members of a group
  - Life tables
  - [[kaplan-meier-estimator|Kaplan–Meier curves]]
  - Survival function
  - Hazard function
- To compare the survival times of two or more groups
  - Log-rank test
- To describe the effect of categorical or quantitative variables on survival
  - Cox proportional hazards regression
  - Parametric survival models
  - Survival trees
  - Survival random forests

### Definitions of common terms in survival analysis

The following terms are commonly used in survival analyses:

- Event: Death, disease occurrence, disease recurrence, recovery, or other experience of interest
- Time: The time from the beginning of an observation period (such as surgery or beginning treatment) to (i) an event, or (ii) end of the study, or (iii) loss of contact or withdrawal from the study.
- Censoring / Censored observation: Censoring occurs when we have some information about individual survival time, but we do not know the survival time exactly. The subject is censored in the sense that nothing is observed or known about that subject after the time of censoring. A censored subject may or may not have an event after the end of observation time.
- Survival function S(t): The probability that a subject survives longer than time t.

### Example: Acute myelogenous leukemia survival data

This example uses the Acute Myelogenous Leukemia survival data set "aml" from the "survival" package in R. The data set is from Miller (1997) and the question is whether the standard course of chemotherapy should be extended ('maintained') for additional cycles.

The aml data set sorted by survival time is shown in the box.

| observation | time (weeks) | status | x |
| --- | --- | --- | --- |
| 12 | 5 | 1 | Nonmaintained |
| 13 | 5 | 1 | Nonmaintained |
| 14 | 8 | 1 | Nonmaintained |
| 15 | 8 | 1 | Nonmaintained |
| 1 | 9 | 1 | Maintained |
| 16 | 12 | 1 | Nonmaintained |
| 2 | 13 | 1 | Maintained |
| 3 | 13 | 0 | Maintained |
| 17 | 16 | 0 | Nonmaintained |
| 4 | 18 | 1 | Maintained |
| 5 | 23 | 1 | Maintained |
| 18 | 23 | 1 | Nonmaintained |
| 19 | 27 | 1 | Nonmaintained |
| 6 | 28 | 0 | Maintained |
| 20 | 30 | 1 | Nonmaintained |
| 7 | 31 | 1 | Maintained |
| 21 | 33 | 1 | Nonmaintained |
| 8 | 34 | 1 | Maintained |
| 22 | 43 | 1 | Nonmaintained |
| 9 | 45 | 0 | Maintained |
| 23 | 45 | 1 | Nonmaintained |
| 10 | 48 | 1 | Maintained |
| 11 | 161 | 0 | Maintained |

- Time is indicated by the variable "time", which is the survival or censoring time
- Event (recurrence of aml cancer) is indicated by the variable "status". 0 = no event (censored), 1 = event (recurrence)
- Treatment group: the variable "x" indicates if maintenance chemotherapy was given

The last observation (11), at 161 weeks, is censored. Censoring indicates that the patient did not have an event (no recurrence of aml cancer). Another subject, observation 3, was censored at 13 weeks (indicated by status=0). This subject was in the study for only 13 weeks, and the aml cancer did not recur during those 13 weeks. It is possible that this patient was enrolled near the end of the study, so that they could be observed for only 13 weeks. It is also possible that the patient was enrolled early in the study, but was lost to follow up or withdrew from the study. The table shows that other subjects were censored at 16, 28, and 45 weeks (observations 17, 6, and 9 with status=0). The remaining subjects all experienced events (recurrence of aml cancer) while in the study. The question of interest is whether recurrence occurs later in maintained patients than in non-maintained patients.

#### Kaplan–Meier plot for the aml data

The survival function *S*(*t*), is the probability that a subject survives longer than time *t*. *S*(*t*) is theoretically a smooth curve, but it is usually estimated using the [[kaplan-meier-estimator|Kaplan–Meier]] (KM) curve. The graph shows the KM plot for the aml data and can be interpreted as follows:

- The *x* axis is time, from zero (when observation began) to the last observed time point.
- The *y* axis is the proportion of subjects surviving. At time zero, 100% of the subjects are alive without an event.
- The solid line (similar to a staircase) shows the progression of event occurrences.
- A vertical drop indicates an event. In the aml table shown above, two subjects had events at five weeks, two had events at eight weeks, one had an event at nine weeks, and so on. These events at five weeks, eight weeks and so on are indicated by the vertical drops in the KM plot at those time points.
- At the far right end of the KM plot there is a tick mark at 161 weeks. The vertical tick mark indicates that a patient was censored at this time. In the aml data table five subjects were censored, at 13, 16, 28, 45 and 161 weeks. There are five tick marks in the KM plot, corresponding to these censored observations.

#### Life table for the aml data

A life table summarizes survival data in terms of the number of events and the proportion surviving at each event time point. The life table for the aml data, created using the R software, is shown.

| time | n.risk | n.event | survival | std.err | lower 95% CI | upper 95% CI |
| --- | --- | --- | --- | --- | --- | --- |
| 5 | 23 | 2 | 0.913 | 0.0588 | 0.8049 | 1 |
| 8 | 21 | 2 | 0.8261 | 0.079 | 0.6848 | 0.996 |
| 9 | 19 | 1 | 0.7826 | 0.086 | 0.631 | 0.971 |
| 12 | 18 | 1 | 0.7391 | 0.0916 | 0.5798 | 0.942 |
| 13 | 17 | 1 | 0.6957 | 0.0959 | 0.5309 | 0.912 |
| 18 | 14 | 1 | 0.646 | 0.1011 | 0.4753 | 0.878 |
| 23 | 13 | 2 | 0.5466 | 0.1073 | 0.3721 | 0.803 |
| 27 | 11 | 1 | 0.4969 | 0.1084 | 0.324 | 0.762 |
| 30 | 9 | 1 | 0.4417 | 0.1095 | 0.2717 | 0.718 |
| 31 | 8 | 1 | 0.3865 | 0.1089 | 0.2225 | 0.671 |
| 33 | 7 | 1 | 0.3313 | 0.1064 | 0.1765 | 0.622 |
| 34 | 6 | 1 | 0.2761 | 0.102 | 0.1338 | 0.569 |
| 43 | 5 | 1 | 0.2208 | 0.0954 | 0.0947 | 0.515 |
| 45 | 4 | 1 | 0.1656 | 0.086 | 0.0598 | 0.458 |
| 48 | 2 | 1 | 0.0828 | 0.0727 | 0.0148 | 0.462 |

The life table summarizes the events and the proportion surviving at each event time point. The columns in the life table have the following interpretation:

- time gives the time points at which events occur.
- n.risk is the number of subjects at risk immediately before the time point, t. Being "at risk" means that the subject has not had an event before time t, and is not censored before or at time t.
- n.event is the number of subjects who have events at time t.
- survival is the proportion surviving, as determined using the [[kaplan-meier-estimator|Kaplan–Meier]] product-limit estimate.
- std.err is the standard error of the estimated survival. The standard error of the Kaplan–Meier product-limit estimate it is calculated using Greenwood's formula, and depends on the number at risk (n.risk in the table), the number of deaths (n.event in the table) and the proportion surviving (survival in the table).
- lower 95% CI and upper 95% CI are the lower and upper 95% confidence bounds for the proportion surviving.

#### Log-rank test: Testing for differences in survival in the aml data

The log-rank test compares the survival times of two or more groups. This example uses a log-rank test for a difference in survival in the maintained versus non-maintained treatment groups in the aml data. The graph shows KM plots for the aml data broken out by treatment group, which is indicated by the variable "x" in the data.

The null hypothesis for a log-rank test is that the groups have the same survival. The expected number of subjects surviving at each time point in each is adjusted for the number of subjects at risk in the groups at each event time. The log-rank test determines if the observed number of events in each group is significantly different from the expected number. The formal test is based on a chi-squared statistic. When the log-rank statistic is large, it is evidence for a difference in the survival times between the groups. The log-rank statistic approximately has a Chi-squared distribution with one degree of freedom, and the p-value is calculated using the Chi-squared test.

For the example data, the log-rank test for difference in survival gives a p-value of p=0.0653, indicating that the treatment groups do not differ significantly in survival, assuming an alpha level of 0.05. The sample size of 23 subjects is modest, so there is little power to detect differences between the treatment groups. The chi-squared test is based on asymptotic approximation, so the p-value should be regarded with caution for small sample sizes.

### Cox proportional hazards (PH) regression analysis

[[kaplan-meier-estimator|Kaplan–Meier curves]] and log-rank tests are most useful when the predictor variable is categorical (e.g., drug vs. [[placebo]]), or takes a small number of values (e.g., drug doses 0, 20, 50, and 100 mg/day) that can be treated as categorical. The log-rank test and KM curves don't work easily with quantitative predictors such as gene expression, white blood count, or age. For quantitative predictor variables, an alternative method is Cox proportional hazards regression analysis. Cox PH models work also with categorical predictor variables, which are encoded as {0,1} indicator or dummy variables. The log-rank test is a special case of a Cox PH analysis, and can be performed using Cox PH software.

#### Example: Cox proportional hazards regression analysis for melanoma

This example uses the melanoma data set from Dalgaard Chapter 14.

Data are in the R package ISwR. The Cox proportional hazards regression using R gives the results shown in the box.

The Cox regression results are interpreted as follows.

- Sex is encoded as a numeric vector (1: female, 2: male). The R summary for the Cox model gives the [[hazard-ratio|hazard ratio]] (HR) for the second group relative to the first group, that is, male versus female.
- coef = 0.662 is the estimated logarithm of the hazard ratio for males versus females.
- exp(coef) = 1.94 = exp(0.662) - The log of the hazard ratio (coef= 0.662) is transformed to the hazard ratio using exp(coef). The summary for the Cox model gives the hazard ratio for the second group relative to the first group, that is, male versus female. The estimated hazard ratio of 1.94 indicates that males have higher risk of death (lower survival rates) than females, in these data.
- se(coef) = 0.265 is the standard error of the log hazard ratio.
- z = 2.5 = coef/se(coef) = 0.662/0.265. Dividing the coef by its standard error gives the z score.
- p=0.013. The p-value corresponding to z=2.5 for sex is p=0.013, indicating that there is a significant difference in survival as a function of sex.

The summary output also gives upper and lower 95% confidence intervals for the hazard ratio: lower 95% bound = 1.15; upper 95% bound = 3.26.

Finally, the output gives p-values for three alternative tests for overall significance of the model:

- Likelihood ratio test = 6.15 on 1 df, p=0.0131
- Wald test = 6.24 on 1 df, p=0.0125
- Score (log-rank) test = 6.47 on 1 df, p=0.0110

These three tests are asymptotically equivalent. For large enough N, they will give similar results. For small N, they may differ somewhat. The last row, "Score (logrank) test" is the result for the log-rank test, with p=0.011, the same result as the log-rank test, because the log-rank test is a special case of a Cox PH regression. The Likelihood ratio test has better behavior for small sample sizes, so it is generally preferred.

#### Cox model using a covariate in the melanoma data

The Cox model extends the log-rank test by allowing the inclusion of additional covariates. This example uses the melanoma data set where the predictor variables include a continuous covariate, the thickness of the tumor (variable name = "thick").

In the histograms, the thickness values are positively skewed and do not have a Gaussian-like, symmetric probability distribution. Regression models, including the Cox model, generally give more reliable results with normally-distributed variables. For this example we may use a logarithmic transform. The log of the thickness of the tumor looks to be more normally distributed, so the Cox models will use log thickness. The Cox PH analysis using R gives the results below.

```
library(ISwR)     # CRAN v2.0-11
library(survival) # CRAN v3.8-3

# Run Cox PH analysis for melanoma data set with covariate log tumor thickness
f <- coxph(Surv(days, status==1) ~ sex + log(thick), data = melanom)

# Display Cox PH output
summary(f)
# Call:
# coxph(formula = Surv(days, status == 1) ~ sex + log(thick), data = melanom)
#
#   n= 205, number of events= 57
#
#              coef exp(coef) se(coef)     z Pr(>|z|)
# sex        0.4580    1.5809   0.2687 1.705   0.0883 .
# log(thick) 0.7809    2.1834   0.1573 4.963 6.94e-07 ***
# ---
# Signif. codes:  0 ‘***’ 0.001 ‘**’ 0.01 ‘*’ 0.05 ‘.’ 0.1 ‘ ’ 1
#
#            exp(coef) exp(-coef) lower .95 upper .95
# sex            1.581     0.6326    0.9337     2.677
# log(thick)     2.183     0.4580    1.6040     2.972
#
# Concordance= 0.749  (se = 0.033 )
# Likelihood ratio test= 33.45  on 2 df,   p=5e-08
# Wald test            = 31  on 2 df,   p=2e-07
# Score (logrank) test = 32.52  on 2 df,   p=9e-08
```

The p-value for all three overall tests (likelihood, Wald, and score) are significant, indicating that the model is significant. The p-value for `\log({\text{thick}})` is 6.94×10−7, with a [[hazard-ratio|hazard ratio]] `HR=\exp({\text{coef}})=2.183`, indicating a strong relationship between the thickness of the tumor and increased risk of death.

By contrast, the p-value for `{\text{sex}}` is now `p=0.088`. The hazard ratio `HR=\exp({\text{coef}})=1.581`, with a 95% confidence interval of 0.9337 to 2.677. Because the confidence interval for HR includes 1, these results indicate that sex makes a smaller contribution to the difference in the HR after controlling for the thickness of the tumor. Examination of graphs of `log({\text{thick}})` by sex and a t-test of `log({\text{thick}})` by sex both indicate that there is a significant difference between men and women in the thickness of the tumor when they first see the clinician.

The Cox model assumes that the hazards are proportional. The proportional hazard assumption may be tested using the R function `cox.zph()` from the survival R package. A p-value which is less than 0.05 indicates that the hazards are not proportional.

```
cox.zph(f)
#            chisq df     p
# sex         1.33  1 0.248
# log(thick)  6.23  1 0.013
# GLOBAL      6.70  2 0.035
```

For the melanoma data, we obtain `p=0.248` for sex. Hence, we cannot reject the null hypothesis of the hazards of sex being proportional.

#### Extensions to Cox models

Cox models can be extended to deal with variations on the simple analysis.

- Stratification. The subjects can be divided into strata, where subjects within a stratum are expected to be relatively more similar to each other than to randomly chosen subjects from other strata. The regression parameters are assumed to be the same across the strata, but a different baseline hazard may exist for each stratum. Stratification is useful for analyses using matched subjects, for dealing with patient subsets, such as different clinics, and for dealing with violations of the proportional hazard assumption.
- Time-varying covariates. Some variables, such as gender and treatment group, generally stay the same in a [[clinical-trial|clinical trial]]. Other clinical variables, such as serum protein levels or dose of concomitant medications may change over the course of a study. Cox models may be extended for such time-varying covariates.

### Tree-structured survival models

The Cox PH regression model is a linear model. It is similar to linear regression and logistic regression. Specifically, these methods assume that a single line, curve, plane, or surface is sufficient to separate groups (alive, dead) or to estimate a quantitative response (survival time).

In some cases alternative partitions give more accurate classification or quantitative estimates. One set of alternative methods are tree-structured survival models, including survival random forests. Tree-structured survival models may give more accurate predictions than Cox models. Examining both types of models for a given data set is a reasonable strategy.

#### Example survival tree analysis

This example of a survival tree analysis uses the R package "rpart". The example is based on 146 stage C prostate cancer patients in the data set stagec in rpart. Rpart and the stagec example are described in Atkinson and Therneau (1997), which is also distributed as a vignette of the rpart package.

The variables in stages are:

- **pgtime**: time to progression, or last follow-up free of progression
- **pgstat**: status at last follow-up (1=progressed, 0=censored)
- **age**: age at diagnosis
- **eet**: early endocrine therapy (1=no, 0=yes)
- **ploidy**: diploid/tetraploid/aneuploid DNA pattern
- **g2**: % of cells in G2 phase
- **grade**: tumor grade (1-4)
- **gleason**: Gleason grade (3-10)

The survival tree produced by the analysis is shown in the figure.

Each branch in the tree indicates a split on the value of a variable. For example, the root of the tree splits subjects with grade < 2.5 versus subjects with grade 2.5 or greater. The terminal nodes indicate the number of subjects in the node, the number of subjects who have events, and the relative event rate compared to the root. In the node on the far left, the values 1/33 indicate that one of the 33 subjects in the node had an event, and that the relative event rate is 0.122. In the node on the far right bottom, the values 11/15 indicate that 11 of 15 subjects in the node had an event, and the relative event rate is 2.7.

#### Survival random forests

An alternative to building a single survival tree is to build many survival trees, where each tree is constructed using a sample of the data, and average the trees to predict survival. This is the method underlying the survival random forest models. Survival random forest analysis is available in the R package "randomForestSRC".

The randomForestSRC package includes an example survival random forest analysis using the data set pbc. This data is from the Mayo Clinic Primary Biliary Cirrhosis (PBC) trial of the liver conducted between 1974 and 1984. In the example, the random forest survival model gives more accurate predictions of survival than the Cox PH model. The prediction errors are estimated by bootstrap re-sampling.

### Deep Learning survival models

Recent advancements in deep representation learning have been extended to survival estimation. The DeepSurv model proposes to replace the log-linear parameterization of the CoxPH model with a multi-layer perceptron. Further extensions like Deep Survival Machines and Deep Cox Mixtures involve the use of latent variable mixture models to model the time-to-event distribution as a mixture of parametric or semi-parametric distributions while jointly learning representations of the input covariates. Deep learning approaches have shown superior performance especially on complex input data modalities such as images and clinical time-series.

## General formulation

### Survival function

The object of primary interest is the **survival function**, conventionally denoted *S*, which is defined as`S(t)=\Pr(T>t)` where *t* is some time, *T* is a random variable denoting the time of death, or more generally any event, and "Pr" stands for probability. That is, the survival function is the probability of observing an event time greater than some specified time *t*. The survival function is also called the *survivor function* or *survivorship function* in problems of biological survival, and the *reliability function* in mechanical survival problems. In the latter case, the reliability function is denoted *R*(*t*).

Usually one assumes *S*(0) = 1, although it could be less than 1 if there is the possibility of immediate death or failure.

The survival function must be non-increasing: *S*(*u*) ≤ *S*(*t*) if *u* ≥ *t*. This property follows directly because *T*>*u* implies *T*>*t*. This reflects the notion that survival to a later age is possible only if all younger ages are attained. Given this property, the lifetime distribution function and event density (*F* and *f* below) are well-defined.

The survival function is usually assumed to approach zero as age increases without bound (i.e., *S*(*t*) → 0 as *t* → ∞), although the limit could be greater than zero if eternal life is possible. For instance, we could apply survival analysis to a mixture of stable and unstable carbon isotopes; unstable isotopes would decay sooner or later, but the stable isotopes would last indefinitely.

### Lifetime distribution function and event density

Related quantities are defined in terms of the survival function.

The **lifetime distribution function**, conventionally denoted *F*, is defined as the complement of the survival function,

`F(t)=\Pr(T\leq t)=1-S(t).` If *F* is differentiable then the derivative, which is the density function of the lifetime distribution, is conventionally denoted *f*,

`f(t)=F'(t)={\frac {d}{dt}}F(t).` The function *f* is sometimes called the **event density**; it is the rate of death or failure events per unit time.

The survival function can be expressed in terms of probability distribution and probability density functions

`S(t)=\Pr(T>t)=\int _{t}^{\infty }f(u)\,du=1-F(t).` Similarly, a survival event density function can be defined as

`s(t)=S'(t)={\frac {d}{dt}}S(t)={\frac {d}{dt}}\int _{t}^{\infty }f(u)\,du={\frac {d}{dt}}[1-F(t)]=-f(t).` In other fields, such as statistical physics, the survival event density function is known as the first passage time density.

### Hazard function and cumulative hazard function

The **hazard function** `h` is defined as the event rate at time `t,` conditional on survival at time `t.`

Synonyms for *hazard function* in different fields include hazard rate, intensity function, force of mortality (demography and actuarial science, denoted by `\mu`), force of failure, or failure rate (engineering, denoted `\lambda`). For example, in actuarial science, `\mu (x)` denotes rate of death for people aged `x`, whereas in reliability engineering `\lambda (t)` denotes rate of failure of components after operation for time `t`.

The hazard function represents the probability that a subject will experience an event in the next small interval of time, divided by the length of this interval, given that they have survived up to time `t`. Formally, this can be written as follows:

`h(t)=\lim _{dt\rightarrow 0}{\frac {\Pr(t<T<t+dt|T>t)}{dt}}=\lim _{dt\rightarrow 0}{\frac {\Pr(t<T<t+dt)}{dt\cdot S(t)}}={\frac {f(t)}{S(t)}}=-{\frac {S'(t)}{S(t)}},`

where Bayes' theorem, and identifying `\Pr(T>t)` as the survival function, has been used in the first equality, and the definition of the density function of the lifetime distribution in the second.

Any function `h` is a hazard function if and only if it satisfies the following properties:

1. `\forall x\geq 0\left(h(x)\geq 0\right)` ,
2. `\int _{0}^{\infty }h(x)dx=\infty` .

In fact, the hazard rate is usually more informative about the underlying mechanism of failure than the other representations of a lifetime distribution.

The hazard function must be non-negative, `\lambda (t)\geq 0`, and its integral over `[0,\infty )` must be infinite, but is not otherwise constrained; it may be increasing or decreasing, non-monotonic, or discontinuous. An example is the bathtub curve hazard function, which is large for small values of `t`, decreasing to some minimum, and thereafter increasing again; this can model the property of some mechanical systems to either fail soon after operation, or much later, as the system ages.

The hazard function can alternatively be represented in terms of the **cumulative hazard function**, conventionally denoted `\Lambda` or `H`:

`\,\Lambda (t)=-\log S(t)` so transposing signs and exponentiating

`\,S(t)=\exp(-\Lambda (t))` or differentiating (with the chain rule)

`{\frac {d}{dt}}\Lambda (t)=-{\frac {S'(t)}{S(t)}}=\lambda (t).` The name "cumulative hazard function" is derived from the fact that

`\Lambda (t)=\int _{0}^{t}\lambda (u)\,du` which is the "accumulation" of the hazard over time.

From the definition of `\Lambda (t)`, we see that it increases without bound as *t* tends to infinity (assuming that `S(t)` tends to zero). This implies that `\lambda (t)` must not decrease too quickly, since, by definition, the cumulative hazard has to diverge. For example, `\exp(-t)` is not the hazard function of any survival distribution, because its integral converges to 1.

The survival function `S(t)`, the cumulative hazard function `\Lambda (t)`, the density `f(t)`, the hazard function `\lambda (t)`, and the lifetime distribution function `F(t)` are related through `S(t)=\exp[-\Lambda (t)]={\frac {f(t)}{\lambda (t)}}=1-F(t),\quad t>0.`

### Quantities derived from the survival distribution

**Future lifetime** at a given time `t_{0}` is the time remaining until death, given survival to age `t_{0}`. Thus, it is `T-t_{0}` in the present notation. The **expected future lifetime** is the expected value of future lifetime. The probability of death at or before age `t_{0}+t`, given survival until age `t_{0}`, is just

`P(T\leq t_{0}+t\mid T>t_{0})={\frac {P(t_{0}<T\leq t_{0}+t)}{P(T>t_{0})}}={\frac {F(t_{0}+t)-F(t_{0})}{S(t_{0})}}.` Therefore, the probability density of future lifetime is

`{\frac {d}{dt}}{\frac {F(t_{0}+t)-F(t_{0})}{S(t_{0})}}={\frac {f(t_{0}+t)}{S(t_{0})}}` and the expected future lifetime is

`{\frac {1}{S(t_{0})}}\int _{0}^{\infty }t\,f(t_{0}+t)\,dt={\frac {1}{S(t_{0})}}\int _{t_{0}}^{\infty }S(t)\,dt,` where the second expression is obtained using integration by parts.

For `t_{0}=0`, that is, at birth, this reduces to the expected lifetime.

In reliability problems, the expected lifetime is called the *mean time to failure*, and the expected future lifetime is called the *mean residual lifetime*.

As the probability of an individual surviving until age *t* or later is *S*(*t*), by definition, the expected number of survivors at age *t* out of an initial population of *n* newborns is *n* × *S*(*t*), assuming the same survival function for all individuals. Thus the expected proportion of survivors is *S*(*t*). If the survival of different individuals is independent, the number of survivors at age *t* has a binomial distribution with parameters *n* and *S*(*t*), and the variance of the proportion of survivors is *S*(*t*) × (1-*S*(*t*))/*n*.

The age at which a specified proportion of survivors remain can be found by solving the equation *S*(*t*) = *q* for *t*, where *q* is the quantile in question. Typically one is interested in the **median lifetime**, for which *q* = 1/2, or other quantiles such as *q* = 0.90 or *q* = 0.99.

## Censoring

Censoring is a form of missing data problem in which time to event is not observed for reasons such as termination of study before all recruited subjects have shown the event of interest or the subject has left the study prior to experiencing an event. Censoring is common in survival analysis.

If only the lower limit *l* for the true event time *T* is known such that *T* > *l*, this is called *right censoring*. Right censoring will occur, for example, for those subjects whose birth date is known but who are still alive when they are lost to follow-up or when the study ends. We generally encounter right-censored data.

If the event of interest has already happened before the subject is included in the study but it is not known when it occurred, the data is said to be *left-censored*. When it can only be said that the event happened between two observations or examinations, this is *interval censoring*.

Left censoring occurs for example when a permanent tooth has already emerged prior to the start of a dental study that aims to estimate its emergence distribution. In the same study, an emergence time is interval-censored when the permanent tooth is present in the mouth at the current examination but not yet at the previous examination. Interval censoring often occurs in HIV/AIDS studies. Indeed, time to HIV seroconversion can be determined only by a laboratory assessment which is usually initiated after a visit to the physician. Then one can only conclude that HIV seroconversion has happened between two examinations. The same is true for the diagnosis of AIDS, which is based on clinical symptoms and needs to be confirmed by a medical examination.

It may also happen that subjects with a lifetime less than some threshold may not be observed at all: this is called *truncation*. Note that truncation is different from left censoring, since for a left censored datum, we know the subject exists, but for a truncated datum, we may be completely unaware of the subject. Truncation is also common. In a so-called *delayed entry* study, subjects are not observed at all until they have reached a certain age. For example, people may not be observed until they have reached the age to enter school. Any deceased subjects in the pre-school age group would be unknown. Left-truncated data are common in actuarial work for life insurance and pensions.

Left-censored data can occur when a person's survival time becomes incomplete on the left side of the follow-up period for the person. For example, in an epidemiological example, we may monitor a patient for an infectious disorder starting from the time when he or she is tested positive for the infection. Although we may know the right-hand side of the duration of interest, we may never know the exact time of exposure to the infectious agent.

## Fitting parameters to data

Survival models can be usefully viewed as ordinary regression models in which the response variable is time. However, computing the likelihood function (needed for fitting parameters or making other kinds of inferences) is complicated by the censoring. The likelihood function for a survival model, in the presence of censored data, is formulated as follows. By definition the likelihood function is the conditional probability of the data given the parameters of the model. It is customary to assume that the data are independent given the parameters. Then the likelihood function is the product of the likelihood of each datum. It is convenient to partition the data into four categories: uncensored, left censored, right censored, and interval censored. These are denoted "unc.", "l.c.", "r.c.", and "i.c." in the equation below.

`L(\theta )=\prod _{T_{i}\in unc.}\Pr(T=T_{i}\mid \theta )\prod _{i\in l.c.}\Pr(T<T_{i}\mid \theta )\prod _{i\in r.c.}\Pr(T>T_{i}\mid \theta )\prod _{i\in i.c.}\Pr(T_{i,l}<T<T_{i,r}\mid \theta ).` For uncensored data, with `T_{i}` equal to the age at death, we have

`\Pr(T=T_{i}\mid \theta )=f(T_{i}\mid \theta ).` For left-censored data, such that the age at death is known to be less than `T_{i}`, we have

`\Pr(T<T_{i}\mid \theta )=F(T_{i}\mid \theta )=1-S(T_{i}\mid \theta ).` For right-censored data, such that the age at death is known to be greater than `T_{i}`, we have

`\Pr(T>T_{i}\mid \theta )=1-F(T_{i}\mid \theta )=S(T_{i}\mid \theta ).` For an interval censored datum, such that the age at death is known to be less than `T_{i,r}` and greater than `T_{i,l}`, we have

`\Pr(T_{i,l}<T<T_{i,r}\mid \theta )=S(T_{i,l}\mid \theta )-S(T_{i,r}\mid \theta ).` An important application where interval-censored data arises is current status data, where an event `T_{i}` is known not to have occurred before an observation time and to have occurred before the next observation time.

## Non-parametric estimation

The [[kaplan-meier-estimator|Kaplan–Meier estimator]] can be used to estimate the survival function. The Nelson–Aalen estimator can be used to provide a non-parametric estimate of the cumulative hazard rate function. These estimators require lifetime data. Periodic case (cohort) and death (and recovery) counts are statistically sufficient to make nonparametric maximum likelihood and least squares estimates of survival functions, without lifetime data.

## Discrete-time survival models

While many parametric models assume a continuous-time, discrete-time survival models can be mapped to a binary classification problem. In a discrete-time survival model the survival period is artificially resampled in intervals where for each interval a binary target indicator is recorded if the event takes place in a certain time horizon. If a binary classifier (potentially enhanced with a different likelihood to take more structure of the problem into account) is calibrated, then the classifier score is the hazard function (i.e. the conditional probability of failure).

Discrete-time survival models are connected to empirical likelihood.

## Distributional Regression

Instead of performing the time-discretization and bringing the data in this long-format, an alternative way is to perform distributional regression over the binned time-axis. This approach can leverage prior data fitted networks (tabular foundation models) to work with small data sets already.

## Goodness of fit

The goodness of fit of survival models can be assessed using scoring rules.

## Cure model

A cure model allows for the possibility that some individuals never experience the event of interest. This creates survival curves that plateau rather than always converging to zero.

A cure model has two linked components

1. Logistic regression for the probability of never experiencing the event:

`\pi (X)=P({\text{cured}}\mid X)`

2. Hazard model E.g. discrete time logistic regression for the conditional event hazard at time t, given susceptibility:

`h(t\mid X)=P(T=t\mid T\geq t,{\text{susceptible}},X)`

So the combined survival function is

`S(t\mid X)=\pi (X)+(1-\pi (X))\prod _{j=1}^{t}(1-h(j\mid X))`

First term: cured individuals, who survive indefinitely.

Second term: susceptible individuals, whose survival declines according to the hazard.

Without a cure model, survival is forced to zero in the limit of large times. With a cure fraction, survival instead converges to

`\lim _{t\to \infty }S(t\mid X)=\pi (X)>0`

## Computer software for survival analysis

The textbook by Kleinbaum has examples of survival analyses using SAS, R, and other packages. The textbooks by Brostrom, Dalgaard and Tableman and Kim give examples of survival analyses using R (or using S, and which run in R).

## Distributions used in survival analysis

- Exponential distribution
- Exponential-logarithmic distribution
- Gamma distribution
- Generalized gamma distribution
- Hypertabastic distribution
- Lindley distribution
- Log-logistic distribution
- Weibull distribution

## Applications

- Credit risk
- False conviction rate of inmates sentenced to death
- Lead times for metallic components in the aerospace industry
- Predictors of criminal recidivism
- Survival distribution of radio-tagged animals
- Time-to-violent death of Roman emperors
- Intertrade waiting times of electronically traded shares on an official public listing
