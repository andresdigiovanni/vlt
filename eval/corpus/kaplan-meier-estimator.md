---
title: Kaplan–Meier estimator
source: https://en.wikipedia.org/wiki/Kaplan%E2%80%93Meier_estimator
revision: 1298262528
license: CC BY-SA 4.0
---

# Kaplan–Meier estimator

The **Kaplan–Meier estimator**, also known as the **product limit estimator**, is a non-parametric statistic used to estimate the survival function from lifetime data. In medical research, it is often used to measure the fraction of patients living for a certain amount of time after treatment. In other fields, Kaplan–Meier estimators may be used to measure the length of time people remain unemployed after a job loss, the time-to-failure of machine parts, or how long fleshy fruits remain on plants before they are removed by frugivores. The estimator is named after Edward L. Kaplan and Paul Meier, who each submitted similar manuscripts to the *Journal of the American Statistical Association*. The journal editor, John Tukey, convinced them to combine their work into one paper, which has been cited more than 34,000 times since its publication in 1958.

The estimator of the survival function `S(t)` (the probability that life is longer than `t`) is given by:

`{\widehat {S}}(t)=\prod \limits _{i:\ t_{i}\leq t}\left(1-{\frac {d_{i}}{n_{i}}}\right),`

with `t_{i}` a time when at least one event happened, *d**i* the *number of events* (e.g., deaths) that happened at time `t_{i}`, and `n_{i}` the *individuals known to have survived* (have not yet had an event or been censored) up to time `t_{i}`.

## Basic concepts

A plot of the Kaplan–Meier estimator is a series of declining horizontal steps which, with a large enough sample size, approaches the true survival function for that population. The value of the survival function between successive distinct sampled observations ("clicks") is assumed to be constant.

An important advantage of the Kaplan–Meier curve is that the method can take into account some types of censored data, particularly *right-censoring*, which occurs if a patient withdraws from a study, is lost to follow-up, or is alive without event occurrence at last follow-up. On the plot, small vertical tick-marks state individual patients whose survival times have been right-censored. When no truncation or censoring occurs, the Kaplan–Meier curve is the complement of the empirical distribution function.

In medical statistics, a typical application might involve grouping patients into categories, for instance, those with Gene A profile and those with Gene B profile. In the graph, patients with Gene B die much quicker than those with Gene A. After two years, about 80% of the Gene A patients survive, but less than half of patients with Gene B.

To generate a Kaplan–Meier estimator, at least two pieces of data are required for each patient (or each subject): the status at last observation (event occurrence or right-censored), and the time to event (or time to censoring). If the survival functions between two or more groups are to be compared, then a third piece of data is required: the group assignment of each subject.

## Problem definition

Let `\tau \geq 0` be a random variable as the time that passes between the start of the possible exposure period, `t_{0}`, and the time that the event of interest takes place, `t_{1}`. As indicated above, the goal is to estimate the survival function `S` underlying `\tau`. Recall that this function is defined as

`S(t)=\operatorname {Prob} (\tau >t)`, where `t=0,1,\dots` is the time.

Let `\tau _{1},\dots ,\tau _{n}\geq 0` be independent, identically distributed random variables, whose common distribution is that of `\tau`: `\tau _{j}` is the random time when some event `j` happened. The data available for estimating `S` is not `(\tau _{j})_{j=1,\dots ,n}`, but the list of pairs `(\,({\tilde {\tau }}_{j},c_{j})\,)_{j=1,\dots ,n}` where for `j\in [n]:=\{1,2,\dots ,n\}`, `c_{j}\geq 0` is a fixed, deterministic integer, the **censoring time** of event `j` and `{\tilde {\tau }}_{j}=\min(\tau _{j},c_{j})`. In particular, the information available about the timing of event `j` is whether the event happened before the fixed time `c_{j}` and if so, then the actual time of the event is also available. The challenge is to estimate `S(t)` given this data.

## Derivation of the Kaplan–Meier estimator

Two derivations of the Kaplan–Meier estimator are shown. Both are based on rewriting the survival function in terms of what is sometimes called **hazard**, or **mortality rates**. However, before doing this it is worthwhile to consider a naive estimator.

### A naive estimator

To understand the power of the Kaplan–Meier estimator, it is worthwhile to first describe a naive estimator of the survival function.

Fix `k\in [n]:=\{1,\dots ,n\}` and let `t>0`. A basic argument shows that the following proposition holds:

**Proposition 1:** If the censoring time `c_{k}` of event `k` exceeds `t` (`c_{k}\geq t`), then `{\tilde {\tau }}_{k}\geq t` if and only if `\tau _{k}\geq t`.

Let `k` be such that `c_{k}\geq t`. It follows from the above proposition that

`\operatorname {Prob} (\tau _{k}\geq t)=\operatorname {Prob} ({\tilde {\tau }}_{k}\geq t).`

Let `X_{k}=\mathbb {I} ({\tilde {\tau }}_{k}\geq t)` and consider only those `k\in C(t):=\{k\,:\,c_{k}\geq t\}`, i.e. the events for which the outcome was not censored before time `t`. Let `m(t)=|C(t)|` be the number of elements in `C(t)`. Note that the set `C(t)` is not random and so neither is `m(t)`. Furthermore, `(X_{k})_{k\in C(t)}` is a sequence of independent, identically distributed Bernoulli random variables with common parameter `S(t)=\operatorname {Prob} (\tau \geq t)`. Assuming that `m(t)>0`, this suggests to estimate `S(t)` using

`{\hat {S}}_{\text{naive}}(t)={\frac {1}{m(t)}}\sum _{k:c_{k}\geq t}X_{k}={\frac {|\{1\leq k\leq n\,:\,{\tilde {\tau }}_{k}\geq t\}|}{|\{1\leq k\leq n\,:\,c_{k}\geq t\}|}}={\frac {|\{1\leq k\leq n\,:\,{\tilde {\tau }}_{k}\geq t\}|}{m(t)}},`

where the second equality follows because `{\tilde {\tau }}_{k}\geq t` implies `c_{k}\geq t`, while the last equality is simply a change of notation.

The quality of this estimate is governed by the size of `m(t)`. This can be problematic when `m(t)` is small, which happens, by definition, when a lot of the events are censored. A particularly unpleasant property of this estimator, that suggests that perhaps it is not the "best" estimator, is that it ignores all the observations whose censoring time precedes `t`. Intuitively, these observations still contain information about `S(t)`: For example, when for many events with `c_{k}<t`, `\tau _{k}<c_{k}` also holds, we can infer that events often happen early, which implies that `\operatorname {Prob} (\tau \leq t)` is large, which, through `S(t)=1-\operatorname {Prob} (\tau \leq t)` means that `S(t)` must be small. However, this information is ignored by this naive estimator. The question is then whether there exists an estimator that makes a better use of all the data. This is what the Kaplan–Meier estimator accomplishes. Note that the naive estimator cannot be improved when censoring does not take place; so whether an improvement is possible critically hinges upon whether censoring is in place.

### The plug-in approach

By elementary calculations,

`{\begin{aligned}S(t)&=\operatorname {Prob} (\tau >t\mid \tau >t-1)\operatorname {Prob} (\tau >t-1)\\[4pt]&=(1-\operatorname {Prob} (\tau \leq t\mid \tau >t-1))\operatorname {Prob} (\tau >t-1)\\[4pt]&=(1-\operatorname {Prob} (\tau =t\mid \tau \geq t))\operatorname {Prob} (\tau >t-1)\\[4pt]&=q(t)S(t-1)\,,\end{aligned}}`

where the second to last equality used that `\tau` is integer valued and for the last line we introduced

`q(t)=1-\operatorname {Prob} (\tau =t\mid \tau \geq t).`

By a recursive expansion of the equality `S(t)=q(t)S(t-1)`, we get

`S(t)=q(t)q(t-1)\cdots q(0).`

Note that here `q(0)=1-\operatorname {Prob} (\tau =0\mid \tau >-1)=1-\operatorname {Prob} (\tau =0)`.

The Kaplan–Meier estimator can be seen as a "plug-in estimator" where each `q(s)` is estimated based on the data and the estimator of `S(t)` is obtained as a product of these estimates.

It remains to specify how `q(s)=1-\operatorname {Prob} (\tau =s\mid \tau \geq s)` is to be estimated. By a similar reasoning that lead to the construction of the naive estimator above, for any `k\in [n]` such that `c_{k}>s`, `\operatorname {Prob} (\tau =s)=\operatorname {Prob} ({\tilde {\tau }}_{k}=s)` and `\operatorname {Prob} (\tau \geq s)=\operatorname {Prob} ({\tilde {\tau }}_{k}\geq s)` both hold. Hence, for any `k\in [n]` such that `c_{k}>s`,

`\operatorname {Prob} (\tau =s|\tau \geq s)=\operatorname {Prob} ({\tilde {\tau }}_{k}=s)/\operatorname {Prob} ({\tilde {\tau }}_{k}\geq s).`

Hence we arrive at the estimator

`{\hat {q}}(s)=1-{\frac {|\{1\leq k\leq n\,:\,c_{k}>s,{\tilde {\tau }}_{k}=s\}|}{|\{1\leq k\leq n\,:\,c_{k}>s,{\tilde {\tau }}_{k}\geq s\}|}}`

(think of estimating the numerator and denominator separately in the definition of the "hazard rate" `\operatorname {Prob} (\tau =s|\tau \geq s)`). The Kaplan–Meier estimator is then given by

`{\hat {S}}(t)=\prod _{s=0}^{t}{\hat {q}}(s).`

The form of the estimator stated at the beginning of the article can be obtained by some further algebra. For this, write `{\hat {q}}(s)=1-d(s)/n(s)` where, using the actuarial science terminology, `d(s)=|\{1\leq k\leq n\,:\,c_{k}>s,{\tilde {\tau }}_{k}=s\}|` is the number of known deaths at time `s`, while `n(s)=|\{1\leq k\leq n\,:\,c_{k}>s,{\tilde {\tau }}_{k}\geq s\}|` is the number of those persons who are alive (and not being censored) at time `s`.

Note that if `d(s)=0`, `{\hat {q}}(s)=1`. This implies that we can leave out from the product defining `{\hat {S}}(t)` all those terms where `d(s)=0`. Then, letting `0\leq t_{1}<t_{2}<\dots <t_{m}` be the times `s` when `d(s)>0`, `d_{i}=d(t_{i})` and `n_{i}=n(t_{i})`, we arrive at the form of the Kaplan–Meier estimator given at the beginning of the article:

`{\hat {S}}(t)=\prod _{i:t_{i}\leq t}\left(1-{\frac {d_{i}}{n_{i}}}\right).`

As opposed to the naive estimator, this estimator can be seen to use the available information more effectively: In the special case mentioned beforehand, when there are many early events recorded, the estimator will multiply many terms with a value below one and will thus take into account that the survival probability cannot be large.

### Derivation as a maximum likelihood estimator

Kaplan–Meier estimator can be derived from maximum likelihood estimation of the discrete hazard function. More specifically given `d_{i}` as the number of events and `n_{i}` the total individuals at risk at time `t_{i}`, discrete hazard rate `h_{i}` can be defined as the probability of an individual with an event at time `t_{i}`. Then survival rate can be defined as:

`S(t)=\prod \limits _{i:\ t_{i}\leq t}(1-h_{i})`

and the likelihood function for the hazard function up to time `t_{i}` is:

`{\mathcal {L}}(h_{j:j\leq i}\mid d_{j:j\leq i},n_{j:j\leq i})=\prod _{j=1}^{i}h_{j}^{d_{j}}(1-h_{j})^{n_{j}-d_{j}}{n_{j} \choose d_{j}}`

therefore the log likelihood will be:

`\log({\mathcal {L}})=\sum _{j=1}^{i}\left(d_{j}\log(h_{j})+(n_{j}-d_{j})\log(1-h_{j})+\log {n_{j} \choose d_{j}}\right)`

finding the maximum of log likelihood with respect to `h_{i}` yields:

`{\frac {\partial \log({\mathcal {L}})}{\partial h_{i}}}={\frac {d_{i}}{{\widehat {h}}_{i}}}-{\frac {n_{i}-d_{i}}{1-{\widehat {h}}_{i}}}=0\Rightarrow {\widehat {h}}_{i}={\frac {d_{i}}{n_{i}}}`

where hat is used to denote maximum likelihood estimation. Given this result, we can write:

`{\widehat {S}}(t)=\prod \limits _{i:\ t_{i}\leq t}\left(1-{\widehat {h}}_{i}\right)=\prod \limits _{i:\ t_{i}\leq t}\left(1-{\frac {d_{i}}{n_{i}}}\right)`

More generally (for continuous as well as discrete survival distributions), the Kaplan-Meier estimator may be interpreted as a nonparametric maximum likelihood estimator.

## Benefits and limitations

The Kaplan–Meier estimator is one of the most frequently used methods of [[survival-analysis|survival analysis]]. The estimate may be useful to examine recovery rates, the probability of death, and the effectiveness of treatment. It is limited in its ability to estimate survival adjusted for covariates; parametric survival models and the Cox proportional hazards model may be useful to estimate covariate-adjusted survival.

The Kaplan-Meier estimator is directly related to the Nelson-Aalen estimator and both maximize the empirical likelihood.

## Statistical considerations

The Kaplan–Meier estimator is a statistic, and several estimators are used to approximate its variance. One of the most common estimators is Greenwood's formula:

`{\widehat {\operatorname {Var} }}\left({\widehat {S}}(t)\right)={\widehat {S}}(t)^{2}\sum _{i:\ t_{i}\leq t}{\frac {d_{i}}{n_{i}(n_{i}-d_{i})}},`

where `d_{i}` is the number of cases and `n_{i}` is the total number of observations, for `t_{i}<t`.

For a 'sketch' of the mathematical derivation of the equation above, click on "show" to reveal

Greenwood's formula is derived by noting that probability of getting `d_{i}` failures out of `n_{i}` cases follows a binomial distribution with failure probability `h_{i}`. As a result for maximum likelihood hazard rate `{\widehat {h}}_{i}=d_{i}/n_{i}` we have `E\left({\widehat {h}}_{i}\right)=h_{i}` and `\operatorname {Var} \left({\widehat {h}}_{i}\right)=h_{i}(1-h_{i})/n_{i}`. To avoid dealing with multiplicative probabilities we compute variance of logarithm of `{\widehat {S}}(t)` and will use the delta method to convert it back to the original variance:

`{\begin{aligned}\operatorname {Var} \left(\log {\widehat {S}}(t)\right)&\sim {\frac {1}{{{\widehat {S}}(t)}^{2}}}\operatorname {Var} \left({\widehat {S}}(t)\right)\Rightarrow \\\operatorname {Var} \left({\widehat {S}}(t)\right)&\sim {{{\widehat {S}}(t)}^{2}}\operatorname {Var} \left(\log {\widehat {S}}(t)\right)\end{aligned}}`

using martingale central limit theorem, it can be shown that the variance of the sum in the following equation is equal to the sum of variances:

`\log {\widehat {S}}(t)=\sum \limits _{i:\ t_{i}\leq t}\log \left(1-{\widehat {h}}_{i}\right)`

as a result we can write:

`{\begin{aligned}\operatorname {Var} ({\widehat {S}}(t))&\sim {{{\widehat {S}}(t)}^{2}}\operatorname {Var} \left(\sum _{i:\ t_{i}\leq t}\log \left(1-{\widehat {h}}_{i}\right)\right)\\&\sim {{{\widehat {S}}(t)}^{2}}\sum \limits _{i:\ t_{i}\leq t}\operatorname {Var} \left(\log \left(1-{\widehat {h}}_{i}\right)\right)\end{aligned}}`

using the delta method once more:

`{\begin{aligned}\operatorname {Var} ({\widehat {S}}(t))&\sim {{{\widehat {S}}(t)}^{2}}\sum _{i:\ t_{i}\leq t}\left({\frac {\partial \log \left(1-{\widehat {h}}_{i}\right)}{\partial {\widehat {h}}_{i}}}\right)^{2}\operatorname {Var} \left({\widehat {h}}_{i}\right)\\&={{{\widehat {S}}(t)}^{2}}\sum _{i:\ t_{i}\leq t}\left({\frac {1}{1-{\widehat {h}}_{i}}}\right)^{2}{\frac {{\widehat {h}}_{i}\left(1-{\widehat {h}}_{i}\right)}{n_{i}}}\\&={{{\widehat {S}}(t)}^{2}}\sum _{i:\ t_{i}\leq t}{\frac {{\widehat {h}}_{i}}{n_{i}\left(1-{\widehat {h}}_{i}\right)}}\\&={{{\widehat {S}}(t)}^{2}}\sum _{i:\ t_{i}\leq t}{\frac {d_{i}}{n_{i}(n_{i}-d_{i})}}\end{aligned}}`

as desired.

---

In some cases, one may wish to compare different Kaplan–Meier curves. This can be done by the log rank test, and the Cox proportional hazards test.

Other statistics that may be of use with this estimator are pointwise confidence intervals, the Hall-Wellner band and the equal-precision band.

## Software

- Mathematica: the built-in function `SurvivalModelFit` creates survival models.
- SAS: The Kaplan–Meier estimator is implemented in the `proc lifetest` procedure.
- R: the Kaplan–Meier estimator is available as part of the `survival` package.
- Stata: the command `sts` returns the Kaplan–Meier estimator.
- Python: the `lifelines` and `scikit-survival` packages each include the Kaplan–Meier estimator.
- MATLAB: the `ecdf` function with the `'function','survivor'` arguments can calculate or plot the Kaplan–Meier estimator.
- StatsDirect: The Kaplan–Meier estimator is implemented in the `Survival Analysis` menu.
- SPSS: The Kaplan–Meier estimator is implemented in the `Analyze > Survival > Kaplan-Meier...` menu.
- Julia: the `Survival.jl` package includes the Kaplan–Meier estimator.
- Epi Info: Kaplan–Meier estimator survival curves and results for the log rank test are obtained with the `KMSURVIVAL` command.
