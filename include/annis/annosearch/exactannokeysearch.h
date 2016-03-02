#pragma once

#include <annis/annosearch/annotationsearch.h>
#include <stx/btree_map>

namespace annis
{

class ExactAnnoKeySearch : public AnnotationKeySearch
{
  using ItAnnoNode = stx::btree_multimap<Annotation, nodeid_t>::const_iterator;
  using ItAnnoKey = std::set<AnnotationKey>::const_iterator;

public:
  /**
   * @brief Find all annotations.
   * @param db
   */
  ExactAnnoKeySearch(const DB& db);
  /**
   * @brief Find annotations by name
   * @param db
   * @param annoName
   */
  ExactAnnoKeySearch(const DB& db, const std::string& annoName);
  ExactAnnoKeySearch(const DB& db, const std::string& annoNamspace, const std::string& annoName);

  virtual ~ExactAnnoKeySearch();

  virtual bool next(Match& result) override;
  virtual void reset() override;

  const std::set<AnnotationKey>& getValidAnnotationKeys()
  {
    if(!validAnnotationKeysInitialized)
    {
      initializeValidAnnotationKeys();
    }
    return validAnnotationKeys;
  }
  
  virtual std::int64_t guessMaxCount() const override;

private:
  const DB& db;

  ItAnnoNode it;
  ItAnnoNode itBegin;
  ItAnnoNode itEnd;

  ItAnnoKey itKeyBegin;
  ItAnnoKey itKeyEnd;

  bool validAnnotationKeysInitialized;
  std::set<AnnotationKey> validAnnotationKeys;

  void initializeValidAnnotationKeys();

};


} // end namespace annis
