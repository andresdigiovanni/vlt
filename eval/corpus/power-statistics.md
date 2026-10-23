---
title: Power (statistics)
source: https://en.wikipedia.org/wiki/Power_(statistics)
revision: 1380970690
license: CC BY-SA 4.0
---

# Power (statistics)

In frequentist statistics, **power** is the probability of detecting an effect (i.e. rejecting the null hypothesis) given that some prespecified effect actually exists using a given test in a given context. In typical use, it is a function of the specific test that is used (including the choice of test statistic and significance level), the sample size (more data tends to provide more power), and the effect size (effects or correlations that are large relative to the variability of the data tend to provide more power).

More formally, in the case of a simple hypothesis test with two hypotheses, the **power of the test** is the probability that the test correctly rejects the null hypothesis (`H_{0}`) when the alternative hypothesis (`H_{1}`) is true. It is commonly denoted by `1-\beta`, where `\beta` is the probability of making a type II error (a false negative) conditional on there being a true effect or association.

## Background

Statistical testing uses data from samples to assess, or make inferences about, a statistical population. For example, we may measure the yields of samples of two varieties of a crop, and use a two sample test to assess whether the mean values of this yield differs between varieties.

Under a frequentist hypothesis testing framework, this is done by calculating a test statistic (such as a t-statistic) for the dataset, which has a known theoretical probability distribution if there is no difference (the so-called null hypothesis). If the actual value calculated on the sample is sufficiently unlikely to arise under the null hypothesis, we say we identified a statistically significant effect.

The threshold for significance can be set small to ensure there is little chance of falsely detecting a non-existent effect. However, failing to identify a significant effect does not imply there was none. If we insist on being careful to avoid false positives, we may create false negatives instead. It may simply be too much to expect that we will be able to find satisfactorily strong evidence of a very subtle difference even if it exists. Statistical power is an attempt to quantify this issue.

In the case of the comparison of the two crop varieties, it enables us to answer questions such as:

- Is there a big danger of two very different varieties producing samples that just happen to look indistinguishable by pure chance?
- How much effort do we need to put into this comparison to avoid that danger?
- How different do these varieties need to be before we can expect to notice a difference?

## Description

Suppose we are conducting a hypothesis test. We define two hypotheses `H_{0}` the null hypothesis, and `H_{1}` the alternative hypothesis. If we design the test such that *α* is the significance level (*α* being the probability of rejecting `H_{0}` when `H_{0}` is in fact true) then the power of the test is 1 − *β* where *β* is the probability of failing to reject `H_{0}` when the alternative `H_{1}` is true.

|  | Probability to reject `H_{0}` | Probability to not reject `H_{0}` |
| --- | --- | --- |
| If `H_{0}` is True | *α* | 1 − *α* |
| If `H_{1}` is True | 1 − *β* (power) | *β* |

To make this more concrete, a typical statistical test would be based on a test statistic *t* calculated from the sampled data, which has a particular probability distribution under `H_{0}`. A desired significance level *α* would then define a corresponding "rejection region" (bounded by certain "critical values"), a set of values *t* is unlikely to take if `H_{0}` was correct. If we reject `H_{0}` in favor of `H_{1}` only when the sample *t* takes those values, we would be able to keep the probability of falsely rejecting `H_{0}` within our desired significance level. At the same time, if `H_{1}` defines its own probability distribution for *t* (the difference between the two distributions being a function of the effect size), the power of the test would be the probability, under `H_{1}`, that the sample *t* falls into our defined rejection region and causes `H_{0}` to be correctly rejected.

Statistical power is one minus the type II error probability and is also the sensitivity of the hypothesis testing procedure to detect a true effect. There is usually a trade-off between demanding more stringent tests (and so, smaller rejection regions) and trying to have a high probability of rejecting the null under the alternative hypothesis. Statistical power may also be extended to the case where multiple hypotheses are being tested based on an experiment or survey. It is thus also common to refer to the **power of a study**, evaluating a scientific project in terms of its ability to answer the research questions they are seeking to answer.

## Applications

The main application of statistical power is "power analysis", a calculation of power usually done before an experiment is conducted using data from pilot studies or a literature review. Power analyses can be used to calculate the minimum sample size required so that one can be reasonably likely to detect an effect of a given size (in other words, producing an acceptable level of power). For example: "How many times do I need to toss a coin to conclude it is rigged by a certain amount?" If resources and thus sample sizes are fixed, power analyses can also be used to calculate the minimum effect size that is likely to be detected.

Funding agencies, [[institutional-review-board|ethics boards]] and research review panels frequently request that a researcher perform a power analysis. An underpowered study is likely to be inconclusive, failing to allow one to choose between hypotheses at the desired significance level, while an overpowered study will spend great expense on being able to report significant effects even if they are tiny and so practically meaningless. If a large number of underpowered studies are done and statistically significant results published, published findings are more likely false positives than true results, contributing to a replication crisis. However, excessive demands for power could be connected to wasted resources and ethical problems, for example the use of a large number of animal test subjects when a smaller number would have been sufficient. It could also induce researchers trying to seek funding to overstate their expected effect sizes, or avoid looking for more subtle interaction effects that cannot be easily detected.

Power analysis is primarily a frequentist statistics tool. In Bayesian statistics, hypothesis testing of the type used in classical power analysis is not done. In the Bayesian framework, one updates his or her prior beliefs using the data obtained in a given study. In principle, a study that would be deemed underpowered from the perspective of hypothesis testing could still be used in such an updating process. However, power remains a useful measure of how much a given experiment size can be expected to refine one's beliefs. A study with low power is unlikely to lead to a large change in beliefs.

In addition, the concept of power is used to make comparisons between different statistical testing procedures: for example, between a parametric test and a nonparametric test of the same hypothesis. Tests may have the same size, and hence the same false positive rates, but different ability to detect true effects. Consideration of their theoretical power proprieties is a key reason for the common use of likelihood ratio tests.

## Rule of thumb for t-test

Lehr's (rough) rule of thumb says that the sample size `n` (for each group) for the common case of a two-sided two-sample t-test with power 80% (`\beta =0.2`) and significance level `\alpha =0.05` should be: `n\approx 16{\frac {s^{2}}{d^{2}}},` where `s^{2}` is an estimate of the population variance and `d=\mu _{1}-\mu _{2}` the to-be-detected difference in the mean values of both samples. This expression can be rearranged, implying for example that 80% power is obtained when looking for a difference in means that exceeds about 4 times the group-wise standard error of the mean.

For a one sample t-test 16 is to be replaced with 8. Other values provide an appropriate approximation when the desired power or significance level are different.

However, a full power analysis should always be performed to confirm and refine this estimate.

## Factors influencing power

Statistical power may depend on a number of factors. Some factors may be particular to a specific testing situation, but in normal use, power depends on the following three aspects that can be potentially controlled by the practitioner:

- the test itself and the statistical significance criterion used
- the magnitude of the effect of interest
- the size and variability of the sample used to detect the effect

For a given test, the **significance criterion** determines the desired degree of rigor, specifying how unlikely it is for the null hypothesis of no effect to be rejected if it is in fact true. The most commonly used threshold is a probability of rejection of 0.05, though smaller values like 0.01 or 0.001 are sometimes used. This threshold then implies that the observation must be at least that unlikely (perhaps by suggesting a sufficiently large estimate of difference) to be considered strong enough evidence against the null. Picking a smaller value to tighten the threshold, so as to reduce the chance of a false positive, would also reduce power (and so increase the chance of a false negative). Some statistical tests will inherently produce better power, albeit often at the cost of requiring stronger assumptions.

The **magnitude of the effect** of interest defines what is being looked for by the test. It can be the expected effect size if it exists, as a scientific hypothesis that the researcher has arrived at and wishes to test. Alternatively, in a more practical context it could be determined by the size the effect must be to be useful, for example that which is required to be clinically significant. An effect size can be a direct value of the quantity of interest (for example, a difference in mean of a particular size), or it can be a standardized measure that also accounts for the variability in the population (such as a difference in means expressed as a multiple of the standard deviation). If the researcher is looking for a larger effect, then it should be easier to find with a given experimental or analytic setup, and so power is higher.

The nature of the **sample** underlies the information being used in the test. This will usually involve the sample size, and the sample variability, if that is not implicit in the definition of the effect size. More broadly, the precision with which the data are measured can also be an important factor (such as the statistical reliability), as well as the design of an experiment or observational study. Ultimately, these factors lead to an expected amount of sampling error. A smaller sampling error could be obtained by larger sample sizes from a less variability population, from more accurate measurements, or from more efficient experimental designs (for example, with the appropriate use of blocking), and such smaller errors would lead to improved power, albeit usually at a cost in resources. How increased sample size translates to higher power is a measure of the efficiency of the test—for example, the sample size required for a given power.

## Discussion

The statistical power of a hypothesis test has an impact on the interpretation of its results. Not finding a result with a more powerful study is stronger evidence against the effect existing than the same finding with a less powerful study. However, this is not completely conclusive. The effect may exist, but be smaller than what was looked for, meaning the study is in fact underpowered and the sample is thus unable to distinguish it from random chance. Many [[clinical-trial|clinical trials]], for instance, have low statistical power to detect differences in adverse effects of treatments, since such effects may only affect a few patients, even if this difference can be important. Conclusions about the probability of actual presence of an effect also should consider more things than a single test, especially as real world power is rarely close to 1.

Indeed, although there are no formal standards for power, many researchers and funding bodies assess power using 0.80 (or 80%) as a standard for adequacy. This convention implies a four-to-one trade off between β-risk and α-risk, as the probability of a type II error β is set as 1 - 0.8 = 0.2, while α, the probability of a type I error, is commonly set at 0.05. Some applications require much higher levels of power. Medical tests may be designed to minimise the number of false negatives (type II errors) produced by loosening the threshold of significance, raising the risk of obtaining a false positive (a type I error). The rationale is that it is better to tell a healthy patient "we may have found something—let's test further," than to tell a diseased patient "all is well."

Power analysis focuses on the correct rejection of a null hypothesis. Alternative concerns may however motivate an experiment, and so lead to different needs for sample size. In many contexts, the issue is less about deciding between hypotheses but rather with getting an estimate of the population effect size of sufficient accuracy. For example, a careful power analysis can tell you that 55 pairs of normally distributed samples with a correlation of 0.5 will be sufficient to grant 80% power in rejecting a null that the correlation is no more than 0.2 (using a one-sided test, α = 0.05). But the typical 95% confidence interval with this sample would be around [0.27, 0.67]. An alternative, albeit related analysis would be required if we wish to be able to measure correlation to an accuracy of ± 0.1, implying a different (in this case, larger) sample size. Alternatively, multiple under-powered studies can still be useful, if appropriately combined through a [[meta-analysis]].

Many statistical analyses involve the estimation of several unknown quantities. In simple cases, all but one of these quantities are nuisance parameters. In this setting, the only relevant power pertains to the single quantity that will undergo formal statistical inference. In some settings, particularly if the goals are more "exploratory", there may be a number of quantities of interest in the analysis. For example, in a multiple regression analysis we may include several covariates of potential interest. In situations such as this where several hypotheses are under consideration, it is common that the powers associated with the different hypotheses differ. For instance, in multiple regression analysis, the power for detecting an effect of a given size is related to the variance of the covariate. Since different covariates will have different variances, their powers will differ as well.

Additional complications arise when we consider these multiple hypotheses together. For example, if we consider a false positive to be making an erroneous null rejection on any one of these hypotheses, our likelihood of this "family-wise error" will be inflated if appropriate measures are not taken. Such measures typically involve applying a higher threshold of stringency to reject a hypothesis (such as with the Bonferroni method), and so would reduce power. Alternatively, there may be different notions of power connected with how the different hypotheses are considered. "Complete power" demands that all true effects are detected across all of the hypotheses, which is a much stronger requirement than the "minimal power" of being able to find at least one true effect, a type of power that might increase with an increasing number of hypotheses.

## *A priori* vs. *post hoc* analysis

Power analysis can either be done before (*a priori* or prospective power analysis) or after (*post hoc* or retrospective power analysis) data are collected. *A priori* power analysis is conducted prior to the research study, and is typically used in estimating sufficient sample sizes to achieve adequate power. *Post-hoc* analysis of "observed power" is conducted after a study has been completed, and uses the obtained sample size and effect size to determine what the power was in the study, assuming the effect size in the sample is equal to the effect size in the population. Whereas the utility of prospective power analysis in experimental design is universally accepted, post hoc power analysis is controversial. Many statisticians have argued that post-hoc power calculations are misleading and essentially meaningless.

## Example

The following is an example that shows how to compute power for a randomized experiment: Suppose the goal of an experiment is to study the effect of a treatment on some quantity, and so we shall compare research subjects by measuring the quantity before and after the treatment, analyzing the data using a one-sided paired t-test, with a significance level threshold of 0.05. We are interested in being able to detect a positive change of size `\theta >0`.

We first set up the problem according to our test. Let `A_{i}` and `B_{i}` denote the pre-treatment and post-treatment measures on subject `i`, respectively. The possible effect of the treatment should be visible in the differences `D_{i}=B_{i}-A_{i},` which are assumed to be independent and identically Normal in distribution, with unknown mean value `\mu _{D}` and variance `\sigma _{D}^{2}`.

Here, it is natural to choose our null hypothesis to be that the expected mean difference is zero, i.e. `H_{0}:\mu _{D}=\mu _{0}=0.` For our one-sided test, the alternative hypothesis would be that there is a positive effect, corresponding to `H_{1}:\mu _{D}=\theta >0.` The test statistic in this case is defined as:

`T_{n}={\frac {{\bar {D}}_{n}-\mu _{0}}{{\hat {\sigma }}_{D}/{\sqrt {n}}}}={\frac {{\bar {D}}_{n}-0}{{\hat {\sigma }}_{D}/{\sqrt {n}}}},`

where `\mu _{0}` is the mean under the null so we substitute in 0, n is the sample size (number of subjects), `{\bar {D}}_{n}` is the sample mean of the difference

`{\bar {D}}_{n}={\frac {1}{n}}\sum _{i=1}^{n}D_{i},`

and `{\hat {\sigma }}_{D}` is the sample standard deviation of the difference.

### Analytic solution

We can proceed according to our knowledge of statistical theory, though in practice for a standard case like this software will exist to compute more accurate answers.

Thanks to t-test theory, we know this test statistic under the null hypothesis follows a Student t-distribution with `n-1` degrees of freedom. If we wish to reject the null at significance level `\alpha =0.05\,`, we must find the critical value `t_{\alpha }` such that the probability of `T_{n}>t_{\alpha }` under the null is equal to `\alpha`. If n is large, the t-distribution converges to the standard normal distribution (thus no longer involving n) and so through use of the corresponding quantile function `\Phi ^{-1}`, we obtain that the null should be rejected if

`T_{n}>t_{\alpha }\approx \Phi ^{-1}(0.95)\approx 1.64\,.`

Now suppose that the alternative hypothesis `H_{1}` is true so `\mu _{D}=\theta`. Then, writing the power as a function of the effect size, `B(\theta )`, we find the probability of `T_{n}` being above `t_{\alpha }` under `H_{1}`.

`{\begin{aligned}B(\theta )&\approx \Pr \left(T_{n}>1.64~{\big |}~\mu _{D}=\theta \right)\\&=\Pr \left({\frac {{\bar {D}}_{n}-0}{{\hat {\sigma }}_{D}/{\sqrt {n}}}}>1.64~{\Big |}~\mu _{D}=\theta \right)\\&=1-\Pr \left({\frac {{\bar {D}}_{n}-0}{{\hat {\sigma }}_{D}/{\sqrt {n}}}}<1.64~{\Big |}~\mu _{D}=\theta \right)\\&=1-\Pr \left({\frac {{\bar {D}}_{n}-\theta }{{\hat {\sigma }}_{D}/{\sqrt {n}}}}<1.64-{\frac {\theta }{{\hat {\sigma }}_{D}/{\sqrt {n}}}}~{\Big |}~\mu _{D}=\theta \right)\\\end{aligned}}`

`{\frac {{\bar {D}}_{n}-\theta }{{\hat {\sigma }}_{D}/{\sqrt {n}}}}` again follows a student-t distribution under `H_{1}`, converging on to a standard normal distribution for large n. The estimated `{\hat {\sigma }}_{D}` will also converge on to its population value `\sigma _{D}` Thus power can be approximated as

`B(\theta )\approx 1-\Phi \left(1.64-{\frac {\theta }{\sigma _{D}/{\sqrt {n}}}}\right).`

According to this formula, the power increases with the values of the effect size `\theta` and the sample size n, and reduces with increasing variability `\sigma _{D}`. In the trivial case of zero effect size, power is at a minimum (infimum) and equal to the significance level of the test `\alpha \,,` in this example 0.05. For finite sample sizes and non-zero variability, it is the case here, as is typical, that power cannot be made equal to 1 except in the trivial case where `\alpha =1` so the null is *always* rejected.

We can invert `B` to obtain required sample sizes:

`{\sqrt {n}}>{\frac {\sigma _{D}}{\theta }}\left(1.64-\Phi ^{-1}\left(1-B(\theta )\right)\right).`

Suppose `\theta =1` and we believe `\sigma _{D}` is around 2, say, then we require for a power of `B(\theta )=0.8`, a sample size

`n>4\left(1.64-\Phi ^{-1}\left(1-0.8\right)\right)^{2}\approx 4\left(1.64+0.84\right)^{2}\approx 24.6.`

### Simulation solution

Alternatively we can use a Monte Carlo simulation method that works more generally. Once again, we return to the assumption of the distribution of `D_{n}` and the definition of `T_{n}`. Suppose we have fixed values of the sample size, variability and effect size, and wish to compute power. We can adopt this process:

1. Generate a large number of sets of `D_{n}` according to the null hypothesis, `N(0,\sigma _{D})`

2. Compute the resulting test statistic `T_{n}` for each set.

3. Compute the `(1-\alpha )`th quantile of the simulated `T_{n}` and use that as an estimate of `t_{\alpha }`.

4. Now generate a large number of sets of `D_{n}` according to the alternative hypothesis, `N(\theta ,\sigma _{D})`, and compute the corresponding test statistics again.

5. Look at the proportion of these simulated alternative `T_{n}` that are above the `t_{\alpha }` calculated in step 3 and so are rejected. This is the power.

This can be done with a variety of software packages. Using this methodology with the values before, setting the sample size to 25 leads to an estimated power of around 0.78. The small discrepancy with the previous section is due mainly to inaccuracies with the normal approximation.

## Power in different disciplines

Several studies have attempted to estimate typical levels of statistical power across different academic fields. One common approach uses [[meta-analysis|meta-analyses]] to assess whether individual studies have sufficient power to detect the average effect size estimated from the meta-analysis itself. This method essentially asks: how likely is each study to detect the consensus effect found in the broader literature? These assessments consistently find low levels of statistical power across many disciplines. For example, using this method median power is 18% in economics, 10% in political science, 36% in psychology, and 15% in ecology and evolutionary biology.

## Extension

### Bayesian power

In the frequentist setting, parameters are assumed to have a specific value which is unlikely to be true. This issue can be addressed by assuming the parameter has a distribution. The resulting power is sometimes referred to as Bayesian power which is commonly used in [[clinical-trial|clinical trial]] design.

### Predictive probability of success

Both frequentist power and Bayesian power use statistical significance as the success criterion. However, statistical significance is often not enough to define success. To address this issue, the power concept can be extended to the concept of predictive probability of success (PPOS). The success criterion for PPOS is not restricted to statistical significance and is commonly used in [[clinical-trial|clinical trial]] designs.

## Software for power and sample size calculations

Numerous free and/or open source programs are available for performing power and sample size calculations. These include

- G\*Power (https://www.gpower.hhu.de/)
- WebPower Free online statistical power analysis (https://webpower.psychstat.org)
- Free and open source online calculators (https://powerandsamplesize.com)
- PowerUp! provides Excel-based functions to determine minimum detectable effect size and minimum required sample size for various experimental and quasi-experimental designs.
- PowerUpR is R package version of PowerUp! and additionally includes functions to determine sample size for various multilevel randomized experiments with or without budgetary constraints.
- R package pwr (https://cran.r-project.org/web/packages/pwr/)
- R package WebPower (https://cran.r-project.org/web/packages/WebPower/index.html)
- R package Spower (https://cran.r-project.org/web/packages/Spower/index.html) for general-purpose power analyses using simulation experiments
- Python package statsmodels (https://www.statsmodels.org/)
