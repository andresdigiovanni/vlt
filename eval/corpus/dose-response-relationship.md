---
title: Dose–response relationship
source: https://en.wikipedia.org/wiki/Dose%E2%80%93response_relationship
revision: 1375556265
license: CC BY-SA 4.0
---

# Dose–response relationship

The **dose–response relationship**, or **exposure–response relationship** describes the magnitude of the response of a biochemical or cell-based assay or an organism, as a function of exposure (or doses) to a stimulus or stressor (usually a chemical) after a certain exposure time. Dose–response relationships can be described by **dose–response curves,** or **concentration-response curves**. This is explained further in the following sections. A **stimulus response function** or **stimulus response curve** is defined more broadly as the response from any type of stimulus, not limited to chemicals.

## Motivation for studying dose–response relationships

Studying dose response, and developing dose–response models, is central to determining "safe", "hazardous" and (where relevant) beneficial levels and dosages for drugs, pollutants, foods, and other substances to which humans or other organisms are exposed. These conclusions are often the basis for public policy. The U.S. Environmental Protection Agency has developed extensive guidance and reports on dose–response modeling and assessment, as well as software. The [[food-and-drug-administration|U.S. Food and Drug Administration]] also has guidance to elucidate dose–response relationships during [[drug-development|drug development]]. Dose-response relationships may be used in individuals or in populations. The adage "the dose makes the poison" reflects how a small amount of a toxin can have no significant effect, while a large amount may be fatal. In populations, dose–response relationships can describe the way groups of people or organisms are affected at different levels of exposure. Dose-response relationships modelled by dose response curves are used extensively in pharmacology and drug development. In particular, the shape of a drug's dose–response curve (quantified by EC50, nH and ymax parameters) reflects the biological activity and strength of the drug.

### Example stimuli and responses

Some example measures for dose–response relationships are shown in the tables below. Each sensory stimulus corresponds with a particular sensory receptor, for instance the nicotinic acetylcholine receptor for nicotine, or the mechanoreceptor for mechanical pressure. However, stimuli (such as temperatures or radiation) may also affect physiological processes beyond sensation (and even give the measurable response of death). Responses can be recorded as continuous data (e.g. force of muscle contraction) or discrete data (e.g. number of deaths).

| Example Stimulus | | Target |
| --- | --- | --- |
| Drug/Toxin dose | Agonist (e.g. nicotine, isoprenaline) | Biochemical receptors, Enzymes, Transporters |
| Antagonist (e.g. ketamine, propranolol) |
| Allosteric modulator (e.g. Benzodiazepine) |
| Temperature | | Temperature receptors |
| Sound levels | | Hair cells |
| Illumination/Light intensity | | Photoreceptors |
| Mechanical pressure | | Mechanoreceptors |
| Pathogen dose (e.g. LPS) | | n/a |
| Radiation intensity | | n/a |

| System Level | Example Response |
| --- | --- |
| Population (Epidemiology) | Death, loss of consciousness |
| Organism/Whole animal (Physiology) | Severity of lesion, blood pressure, heart rate, extent of movement, attentiveness, EEG data |
| Organ/Tissue | ATP production, proliferation, muscle contraction, bile production, cell death |
| Cell (Cell biology, Biochemistry) | ATP production, calcium signals, morphology, mitosis |

## Analysis and creation of dose–response curves

### Construction of dose–response curves

A **dose–response curve** is a coordinate graph relating the magnitude of a dose (stimulus) to the response of a biological system. A number of effects (or [[clinical-endpoint|endpoints]]) can be studied. The applied dose is generally plotted on the X axis and the response is plotted on the Y axis. In some cases, it is the logarithm of the dose that is plotted on the X axis. The curve is typically sigmoidal, with the steepest portion in the middle. Biologically based models using dose are preferred over the use of log(dose) because the latter can visually imply a threshold dose when in fact there is none.

Statistical analysis of dose–response curves may be performed by regression methods such as the probit model or logit model, or other methods such as the Spearman–Kärber method. Empirical models based on nonlinear regression are usually preferred over the use of some transformation of the data that linearizes the dose-response relationship. Minimal Neural network (machine learning) models have also been applied to dose–response analysis, including model based on a single, interpretable artificial neuron.

Typical experimental design for measuring dose-response relationships are organ bath preparations, ligand binding assays, functional assays, and clinical drug trials.

Specific to response to doses of radiation the Health Physics Society (in the United States) has published a documentary series on the origins of the linear no-threshold (LNT) model though the society has not adopted a policy on LNT."

#### Hill equation

Due to cooperative binding, logarithmic dose–response curves are generally sigmoidal-shape and monotonic and can be fit to a classical Hill equation. The Hill equation is a logistic function with respect to the logarithm of the dose and is similar to a logit model. A generalized model for multiphasic cases has also been suggested.

The Hill equation is the following formula, where `E` is the magnitude of the response, `{\ce {[A]}}` is the drug concentration (or equivalently, stimulus intensity) and `\mathrm {EC} _{50}` is the drug concentration that produces a 50% maximal response and `n` is the Hill coefficient.

`{\frac {E}{E_{\mathrm {max} }}}={\frac {[A]^{n}}{{\text{EC}}_{50}^{n}+[A]^{n}}}={\frac {1}{1+\left({\frac {\mathrm {EC} _{50}}{[A]}}\right)^{n}}}`

The parameters of the dose response curve reflect measures of potency (such as EC50, IC50, ED50, etc.) and measures of efficacy (such as tissue, cell or population response).

A commonly used dose–response curve is the EC50 curve, the half maximal effective concentration, where the EC50 point is defined as the inflection point of the curve.

Dose response curves are typically fitted to the Hill equation.

The first point along the graph where a response above zero (or above the control response) is reached is usually referred to as a threshold dose. For most beneficial or recreational drugs, the desired effects are found at doses slightly greater than the threshold dose. At higher doses, undesired side effects appear and grow stronger as the dose increases. The more potent a particular substance is, the steeper this curve will be. In quantitative situations, the Y-axis often is designated by percentages, which refer to the percentage of exposed individuals registering a standard response (which may be death, as in LD50). Such a curve is referred to as a quantal dose–response curve, distinguishing it from a graded dose–response curve, where response is continuous (either measured, or by judgment).

The Hill equation can be used to describe dose–response relationships, for example ion channel-open-probability vs. ligand concentration.

Dose is usually in milligrams, micrograms, or grams per kilogram of body-weight for oral exposures or milligrams per cubic meter of ambient air for inhalation exposures. Other dose units include moles per body-weight, moles per animal, and for dermal exposure, moles per square centimeter.

#### Emax model

The Emax model is a generalization of the Hill equation where an effect can be set for zero dose. Using the same notation as above, we can express the model as:

`E=E_{0}+{\frac {{[A]}^{n}\times {E_{\mathrm {max} }}}{{[A]}^{n}+\mathrm {EC} _{50}^{n}}}`

Compare with a rearrangement of Hill:

`E_{\mathrm {hill} }={\frac {{[A]}^{n}\times {E_{\mathrm {max} }}}{{[A]}^{n}+\mathrm {EC} _{50}^{n}}}`

The Emax model is the single most common non-linear model for describing dose-response relationship in drug development.

## Shape of dose-response curve

The shape of dose-response curve typically depends on the topology of the targeted reaction network. While the shape of the curve is often monotonic, in some cases non-monotonic dose response curves can be seen.

## Limitations

The concept of linear dose–response relationship, thresholds, and all-or-nothing responses may not apply to non-linear situations. A threshold model or linear no-threshold model may be more appropriate, depending on the circumstances. A recent critique of these models as they apply to endocrine disruptors argues for a substantial revision of testing and toxicological models at low doses because of observed non-monotonicity, i.e. U-shaped dose/response curves.

Dose–response relationships generally depend on the exposure time and exposure route (e.g., inhalation, dietary intake); quantifying the response after a different exposure time or for a different route leads to a different relationship and possibly different conclusions on the effects of the stressor under consideration. This limitation is caused by the complexity of biological systems and the often unknown biological processes operating between the external exposure and the adverse cellular or tissue response.

## Schild analysis

Schild analysis may also provide insights into the effect of drugs.
